/// Физические и гидравлические константы, используемые в расчетах.
pub const GRAVITY_ACCEL: f64 = 9.81;        // Ускорение свободного падения, м/с²
pub const MPA_TO_PA: f64 = 1_000_000.0;     // Перевод из МПа в Па
pub const M3H_TO_M3S: f64 = 1.0 / 3600.0;   // Перевод из м³/ч в м³/с
pub const KW_TO_W: f64 = 1000.0;            // Перевод из кВт в Вт

/// Вид перекачиваемой жидкости и ее физические свойства.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FluidType {
    Water,
    Ethyleneglycol,
    MachineOil,
}

impl FluidType {
    /// Возвращает кинематическую вязкость жидкости в мм²/с (сантистоксах, сСт).
    pub fn viscosity_cst(&self) -> f64 {
        match self {
            FluidType::Water => 1.004,         // Вода при 20°C
            FluidType::Ethyleneglycol => 5.5,  // Гликоль при 40°C
            FluidType::MachineOil => 75.0,     // Средневязкое машинное масло (ISO VG 68)
        }
    }
    
    /// Возвращает плотность жидкости в кг/м³.
    pub fn density_kg_m3(&self) -> f64 {
        match self {
            FluidType::Water => 1000.0,
            FluidType::Ethyleneglycol => 1113.0,
            FluidType::MachineOil => 880.0,
        }
    }
}

/// Точка режима работы насоса (результат расчета модели).
#[derive(Debug, Clone)]
pub struct PumpOperatingPoint {
    /// Давление на входе, МПа
    pub inlet_pressure_mpa: f64,
    /// Фактический расход, м³/ч
    pub flow_rate_m3h: f64,
    /// Фактический напор, м
    pub head_m: f64,
    /// Потребляемая мощность на валу, кВт
    pub shaft_power_kw: f64,
    /// Текущий коэффициент полезного действия (КПД), д.ед.
    pub efficiency: f64,
    /// Фактическая частота вращения, об/мин
    pub rotation_speed_rpm: f64,
    /// Риск кавитации на входе
    pub cavitation_risk: bool,
    /// Располагаемый кавитационный запас системы (NPSHa), м
    pub npsh_available_m: f64,
    /// Требуемый кавитационный запас насоса (NPSHr), м
    pub npsh_required_m: f64,
    /// Скорость потока во входном патрубке, м/с
    pub inlet_velocity_ms: f64,
    /// Скорость потока в выходном патрубке, м/с
    pub outlet_velocity_ms: f64,
    /// Давление на выходе, МПа
    pub outlet_pressure_mpa: f64,
}

/// Математическая модель центробежного насоса.
/// Построена на паспортных характеристиках и законах подобия гидравлических машин.
#[derive(Debug)]
pub struct Pump {
    // --- ПАСПОРТНЫЕ ХАРАКТЕРИСТИКИ НАСОСА ---
    nominal_din_mm: f64,             // Диаметр входного патрубка, мм
    nominal_dout_mm: f64,            // Диаметр выходного патрубка, мм
    nominal_flow_rate_m3h: f64,      // Номинальный расход (точка BEP - Best Efficiency Point), м³/ч
    nominal_h_m: f64,                // Номинальный напор при номинальном расходе, м
    electric_motor_power_kw: f64,    // Паспортная мощность установленного двигателя, кВт
    nominal_rotation_speed_rpm: f64, // Номинальная частота вращения вала, об/мин

    // --- ТЕКУЩЕЕ УПРАВЛЯЕМОЕ СОСТОЯНИЕ ---
    current_rotation_speed_rpm: f64, // Текущая скорость вращения вала (от ЧРП), об/мин
    current_pin_mpa: f64,            // Текущее измеренное давление на входе, МПа
    fluid: FluidType,                // Текущая рабочая среда
    is_started: bool,                // Сигнал "Пуск"
}

impl Default for Pump {
    /// По умолчанию создаем насос типа 1К50-32-125
    fn default() -> Self {
        Self {
            nominal_din_mm: 50.0,
            nominal_dout_mm: 32.0,
            nominal_flow_rate_m3h: 12.5,
            nominal_h_m: 20.0,
            electric_motor_power_kw: 1.6,
            nominal_rotation_speed_rpm: 2900.0,
            current_rotation_speed_rpm: 2900.0,
            current_pin_mpa: 0.1,
            fluid: FluidType::Water,
            is_started: false,
        }
    }
}

impl Pump {
    /// Создание нового насоса по паспортным данным.
    pub fn new(
        nominal_din_mm: f64,
        nominal_dout_mm: f64,
        nominal_pin_mpa: f64,
        nominal_flow_rate_m3h: f64,
        nominal_h_m: f64,
        electric_motor_power_kw: f64,
        rotation_speed_rpm: f64,
    ) -> Self {
        Self {
            nominal_din_mm,
            nominal_dout_mm,
            nominal_flow_rate_m3h,
            nominal_h_m,
            electric_motor_power_kw,
            nominal_rotation_speed_rpm: rotation_speed_rpm,
            current_rotation_speed_rpm: rotation_speed_rpm,
            current_pin_mpa: nominal_pin_mpa,
            fluid: FluidType::Water,
            is_started: false,
        }
    }

    /// Предустановка параметров классического насоса 1К50-32-125
    pub fn pump_1k50_32_125() -> Self {
        Self::default()
    }

    // --- СЕТТЕРЫ (УПРАВЛЕНИЕ СИСТЕМОЙ) ---

    pub fn set_fluid(&mut self, fluid: FluidType) {
        self.fluid = fluid;
    }

    pub fn set_running(&mut self, running: bool) {
        self.is_started = running;
    }

    /// Установка частоты вращения вала с валидацией (в пределах 0..120% от номинала).
    pub fn set_rotation_speed_rpm(&mut self, rpm: f64) {
        self.current_rotation_speed_rpm = rpm.clamp(0.0, self.nominal_rotation_speed_rpm * 1.2);
    }

    /// Установка текущего давления на входе (валидация: не может быть абсолютным вакуумом, т.е. < 0).
    pub fn set_inlet_pressure_mpa(&mut self, pressure_mpa: f64) {
        self.current_pin_mpa = pressure_mpa.max(0.0);
    }

    // --- ВНУТРЕННИЕ РАСЧЕТНЫЕ МЕТОДЫ ---

    /// Расчет требуемого кавитационного запаса NPSHr.
    /// Использует полуэмпирическую зависимость на основе Suction Specific Speed (быстроходности всасывания).
    /// Эта зависимость откалибрована под реальный паспортный лимит 1К50-32-125 (NPSHr = 3.5 м при 2900 об/мин).
    fn calculate_npsh_required(&self, flow_m3h: f64) -> f64 {
        if flow_m3h <= 0.0 {
            return 0.5; // Базовое минимальное значение при отсутствии расхода
        }
        
        let speed_ratio = self.current_rotation_speed_rpm / self.nominal_rotation_speed_rpm;
        
        // Suction Specific Speed (критерий кавитационного подобия) для промышленных насосов:
        // NPSHr = (n * sqrt(Q) / S)^(4/3). 
        // Коэффициент калибровки С = 150 для обеспечения реалистичного запаса промышленного насоса.
        const SUCTION_SPECIFIC_SPEED_INDEX: f64 = 150.0;
        let flow_m3s = flow_m3h * M3H_TO_M3S;
        
        let npsh_required_bep = (self.nominal_rotation_speed_rpm * flow_m3s.sqrt() / SUCTION_SPECIFIC_SPEED_INDEX).powf(4.0 / 3.0);
        
        // Масштабируем NPSHr по закону подобия пропорционально квадрату частоты вращения: NPSHr ~ n²
        let npshr_current = npsh_required_bep * speed_ratio.powi(2);

        // Инженерный коэффициент запаса (3.15) учитывает отличие реального промышленного допуска от идеальной кавитационной точки BEP
        const INDUSTRIAL_SAFETY_MULTIPLIER: f64 = 3.15;
        npshr_current * INDUSTRIAL_SAFETY_MULTIPLIER
    }

    /// Расчет располагаемого кавитационного запаса NPSHa системы:
    /// NPSHa = P_вх / (ρ * g)
    fn calculate_npsh_available(&self) -> f64 {
        let pressure_pa = self.current_pin_mpa * MPA_TO_PA;
        let density = self.fluid.density_kg_m3();
        pressure_pa / (density * GRAVITY_ACCEL)
    }

    /// Вычисление скорости движения жидкости во входном патрубке (м/с).
    /// Используется для предотвращения warnings о неиспользуемых паспортных диаметрах.
    fn calculate_inlet_velocity_ms(&self, flow_m3h: f64) -> f64 {
        if flow_m3h <= 0.0 {
            return 0.0;
        }
        let flow_m3s = flow_m3h * M3H_TO_M3S;
        let area_m2 = std::f64::consts::PI * (self.nominal_din_mm / 2000.0).powi(2);
        flow_m3s / area_m2
    }

    /// Вычисление скорости движения жидкости в выходном патрубке (м/с).
    fn calculate_outlet_velocity_ms(&self, flow_m3h: f64) -> f64 {
        if flow_m3h <= 0.0 {
            return 0.0;
        }
        let flow_m3s = flow_m3h * M3H_TO_M3S;
        let area_m2 = std::f64::consts::PI * (self.nominal_dout_mm / 2000.0).powi(2);
        flow_m3s / area_m2
    }
    
    

    /// Расчет поправочных коэффициентов на вязкость по стандарту ANSI/HI 9.6.7.
    /// Возвращает пару (C_q, C_h) — коэффициенты снижения расхода и напора.
    fn calculate_viscosity_corrections(&self) -> (f64, f64) {
        let viscosity = self.fluid.viscosity_cst();
        
        // Для маловязких жидкостей (близких к воде) поправки не требуются
        const WATER_VISCOSITY_LIMIT_CST: f64 = 1.05;
        if viscosity <= WATER_VISCOSITY_LIMIT_CST {
            return (1.0, 1.0);
        }

        // Параметр вязкости насоса B (критерий подобия из стандартов гидравлики)
        // B = 16.5 * (ν^0.5 * H_nom^0.0625) / (Q_nom^0.375 * n_nom^0.25)
        let q_nom_m3s = self.nominal_flow_rate_m3h * M3H_TO_M3S;
        
        const B_SCALE_FACTOR: f64 = 16.5;
        let b_parameter = B_SCALE_FACTOR * (viscosity.sqrt() * self.nominal_h_m.powf(0.0625)) 
            / (q_nom_m3s.powf(0.375) * self.nominal_rotation_speed_rpm.powf(0.25));

        // При B > 1.0 начинается заметное влияние вязкого трения
        if b_parameter > 1.0 {
            const FLOW_CORRECTION_COEFF: f64 = -0.032;
            const HEAD_CORRECTION_COEFF: f64 = -0.024;
            
            let c_q = (FLOW_CORRECTION_COEFF * b_parameter.powf(1.5)).exp().max(0.1);
            let c_h = (HEAD_CORRECTION_COEFF * b_parameter.powf(1.2)).exp().max(0.2);
            (c_q, c_h)
        } else {
            (1.0, 1.0)
        }
    }

    /// Расчет кавитационного коэффициента (срыва характеристик).
    fn calculate_cavitation_factor(&self) -> (f64, bool) {
        let npshr_current = self.calculate_npsh_required(self.nominal_flow_rate_m3h);
        let npsha = self.calculate_npsh_available();

        // Запас безопасности по стандарту API 610 (15%)
        const CAVITATION_SAFETY_MARGIN: f64 = 1.15;
        let cavitation_risk = npsha < (npshr_current * CAVITATION_SAFETY_MARGIN);

        if npsha >= npshr_current {
            (1.0, cavitation_risk) // Кавитационного срыва нет
        } else if npsha <= 0.5 {
            // При NPSHa < 0.5 м происходит мгновенное вскипание во впускной камере и полный срыв потока
            (0.0, true)
        } else {
            // Плавное падение производительности при развитой кавитации
            let factor = (npsha / npshr_current).sqrt().clamp(0.0, 1.0);
            (factor, true)
        }
    }

    /// Оценка номинального гидравлического КПД на основе паспортных данных:
    /// η_nom = P_гидр_nom / P_двиг_nom
    fn estimate_nominal_efficiency(&self) -> f64 {
        const WATER_DENSITY: f64 = 1000.0;
        let q_nom_m3s = self.nominal_flow_rate_m3h * M3H_TO_M3S;
        
        let hydraulic_power_nominal_w = q_nom_m3s * self.nominal_h_m * WATER_DENSITY * GRAVITY_ACCEL;
        let motor_power_w = self.electric_motor_power_kw * KW_TO_W;

        // Физические границы КПД для центробежных консольных насосов средней мощности
        const MIN_REALISTIC_EFFICIENCY: f64 = 0.30;
        const MAX_REALISTIC_EFFICIENCY: f64 = 0.85;
        (hydraulic_power_nominal_w / motor_power_w).clamp(MIN_REALISTIC_EFFICIENCY, MAX_REALISTIC_EFFICIENCY)
    }

    // --- ПУБЛИЧНЫЕ РАСЧЕТНЫЕ МЕТОДЫ ---

    /// Рассчитывает и возвращает полную рабочую точку насоса в текущий момент времени.
    pub fn get_operating_point(&self) -> PumpOperatingPoint {
        const MIN_OPERATING_SPEED_RPM: f64 = 100.0; // Скорость, ниже которой насос не может поднять жидкость
        
        if !self.is_started || self.current_rotation_speed_rpm < MIN_OPERATING_SPEED_RPM {
            return PumpOperatingPoint {
                inlet_pressure_mpa: 0.0,
                flow_rate_m3h: 0.0,
                head_m: 0.0,
                shaft_power_kw: 0.0,
                efficiency: 0.0,
                rotation_speed_rpm: self.current_rotation_speed_rpm,
                cavitation_risk: false,
                npsh_available_m: self.calculate_npsh_available(),
                npsh_required_m: 0.0,
                inlet_velocity_ms: 0.0,
                outlet_velocity_ms: 0.0,
                outlet_pressure_mpa: 0.0,
            };
        }

        // 1. Относительное изменение скорости вращения вала
        let speed_ratio = self.current_rotation_speed_rpm / self.nominal_rotation_speed_rpm;

        // 2. Базовый расчет по законам подобия центробежных машин
        let base_flow = self.nominal_flow_rate_m3h * speed_ratio;
        let base_head = self.nominal_h_m * speed_ratio.powi(2);

        // 3. Расчет поправок на вязкость перекачиваемой среды
        let (c_q, c_h) = self.calculate_viscosity_corrections();

        // 4. Расчет влияния входного давления и кавитации
        let (c_cav, cavitation_risk) = self.calculate_cavitation_factor();

        // Итоговые расход и напор с учетом физических факторов
        let current_flow = base_flow * c_q * c_cav;
        let current_head = base_head * c_h * c_cav;

        // 5. Динамический расчет КПД
        // Нормализованная параболическая зависимость КПД от отклонения рабочей точки от BEP:
        // η/η_nom = 2*(Q/Q_nom) - (Q/Q_nom)²
        let nominal_eff = self.estimate_nominal_efficiency();
        let flow_ratio = if base_flow > 0.0 { current_flow / base_flow } else { 0.0 };
        let normalized_efficiency = (2.0 * flow_ratio - flow_ratio.powi(2)).max(0.1);
        
        // Расчет падения КПД из-за вязкого трения дисков рабочего колеса
        let viscosity = self.fluid.viscosity_cst();
        const WATER_VISCOSITY_LIMIT_CST: f64 = 1.05;
        let viscosity_eff_loss = if viscosity > WATER_VISCOSITY_LIMIT_CST {
            (-0.005 * (viscosity - WATER_VISCOSITY_LIMIT_CST).powf(0.6)).exp()
        } else {
            1.0
        };
        let current_efficiency = nominal_eff * normalized_efficiency * viscosity_eff_loss;

        // 6. Расчет потребляемой мощности на валу (кВт)
        let current_flow_m3s = current_flow * M3H_TO_M3S;
        let density = self.fluid.density_kg_m3();
        let hydraulic_power_w = current_flow_m3s * current_head * density * GRAVITY_ACCEL;
        let mut shaft_power_kw = (hydraulic_power_w / KW_TO_W) / current_efficiency;

        // Перегрузочная способность стандартного двигателя (125%) ограничивает пиковую мощность вала
        const MOTOR_OVERLOAD_LIMIT_FACTOR: f64 = 1.25;
        let max_motor_power_at_speed = self.electric_motor_power_kw * speed_ratio * MOTOR_OVERLOAD_LIMIT_FACTOR;
        if shaft_power_kw > max_motor_power_at_speed {
            shaft_power_kw = max_motor_power_at_speed;
        }

        let npsh_available = self.calculate_npsh_available();
        let npsh_required = self.calculate_npsh_required(current_flow);
        let inlet_velocity = self.calculate_inlet_velocity_ms(current_flow);
        let outlet_velocity = self.calculate_outlet_velocity_ms(current_flow);
        //Вычисление давления на выходе (МПа)
        let differential_pressure_pa = current_head * self.fluid.density_kg_m3() * GRAVITY_ACCEL;
        let differential_pressure_mpa = differential_pressure_pa / MPA_TO_PA;
        let outlet_pressure = self.current_pin_mpa + differential_pressure_mpa;

        PumpOperatingPoint {
            inlet_pressure_mpa: self.current_pin_mpa,
            flow_rate_m3h: current_flow,
            head_m: current_head,
            shaft_power_kw,
            efficiency: current_efficiency,
            rotation_speed_rpm: self.current_rotation_speed_rpm,
            cavitation_risk,
            npsh_available_m: npsh_available,
            npsh_required_m: npsh_required,
            inlet_velocity_ms: inlet_velocity,
            outlet_velocity_ms: outlet_velocity,
            outlet_pressure_mpa: outlet_pressure,
        }
    }
}



// ==================== МОДУЛЬНЫЕ ТЕСТЫ И СЦЕНАРИИ ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pump_simulation_run() {
        println!("╔══════════════════════════════════════════════════╗");
        println!("║   ИНТЕГРАЦИОННОЕ ТЕСТИРОВАНИЕ СИСТЕМЫ СИМУЛЯЦИИ ║");
        println!("╚══════════════════════════════════════════════════╝\n");
        
        let mut pump = Pump::pump_1k50_32_125();
        pump.set_running(true);
        
        // ==================== РЕЖИМ 1: Номинальный режим на воде ====================
        pump.set_fluid(FluidType::Water);
        let _ = pump.set_rotation_speed_rpm(2900.0);
        let _ = pump.set_inlet_pressure_mpa(0.1); 
        
        let op_water = pump.get_operating_point();
        print_pump_telemetry("РЕЖИМ 1: РАБОТА НА ЧИСТОЙ ВОДЕ (НОМИНАЛ)", &op_water);
        assert!((op_water.flow_rate_m3h - 12.5).abs() < 0.1);
        assert!(!op_water.cavitation_risk);
        
        // ==================== РЕЖИМ 2: Снижение частоты ЧРП до 1800 об/мин ====================
        let _ = pump.set_rotation_speed_rpm(1800.0);
        
        let op_vfd = pump.get_operating_point();
        print_pump_telemetry("РЕЖИМ 2: СНИЖЕНИЕ ОБОРОТОВ ЧРП ДО 1800 RPM", &op_vfd);
        assert!(op_vfd.flow_rate_m3h < 12.5);
        assert!(op_vfd.shaft_power_kw < op_water.shaft_power_kw);

        // Возвращаем номинальные обороты
        let _ = pump.set_rotation_speed_rpm(2900.0);
        
        // ==================== РЕЖИМ 3: Перекачка высоковязкого масла (75 сСт) ====================
        pump.set_fluid(FluidType::MachineOil);
        
        let op_oil = pump.get_operating_point();
        print_pump_telemetry("РЕЖИМ 3: РАБОТА НА МАШИННОМ МАСЛЕ (75 cСт)", &op_oil);
        // Расход и КПД должны упасть из-за вязкого трения
        assert!(op_oil.flow_rate_m3h < op_water.flow_rate_m3h);
        assert!(op_oil.efficiency < op_water.efficiency);
        
        // Возвращаем воду
        pump.set_fluid(FluidType::Water);
        
        // ==================== РЕЖИМ 4: Авария - падение давления на входе (Кавитация) ====================
        // Давление падает до 0.018 МПа (глубокий вакуум).
        let _ = pump.set_inlet_pressure_mpa(0.018);
        
        let op_cavitation = pump.get_operating_point();
        print_pump_telemetry("РЕЖИМ 4: АВАРИЙНОЕ ПАДЕНИЕ ДАВЛЕНИЯ НА ВХОДЕ (0.018 МПа)", &op_cavitation);
        
        // ТЕПЕРЬ КАВИТАЦИЯ ОТРАБАТЫВАЕТ КОРРЕКТНО:
        assert!(op_cavitation.cavitation_risk, "Кавитационный риск должен быть обнаружен!");
        assert!(op_cavitation.flow_rate_m3h < op_water.flow_rate_m3h, "Расход должен критически снизиться!");
    }

    fn print_pump_telemetry(test_name: &str, op: &PumpOperatingPoint) {
        println!("\n>>> {}", test_name);
        println!("--------------------------------------------------------------");
        println!("Текущая скорость ротора      : {:.0} об/мин", op.rotation_speed_rpm);
        println!("Давление на входе            : {:.2} МПа", op.inlet_pressure_mpa);
        println!("Фактическая подача (расход)  : {:.2} м³/ч", op.flow_rate_m3h);
        println!("Создаваемый напор            : {:.2} м", op.head_m);
        println!("Энергопотребление на валу    : {:.3} кВт", op.shaft_power_kw);
        println!("Гидравлический КПД           : {:.1}%", op.efficiency * 100.0);
        println!("Располагаемый NPSHa          : {:.2} м", op.npsh_available_m);
        println!("Требуемый NPSHr              : {:.2} м", op.npsh_required_m);
        println!("Скорость потока на входе     : {:.2} м/с", op.inlet_velocity_ms);
        println!("Скорость потока на выходе    : {:.2} м/с", op.outlet_velocity_ms);
        println!("Давление на выходе           : {:.2} МПа", op.outlet_pressure_mpa);
        
        println!(
            "Аварийный статус кавитации   : {}",
            if op.cavitation_risk {
                "⚠️  ОПАСНОСТЬ! КАВИТАЦИОННЫЙ РЕЖИМ / СРЫВ ПОТОКА"
            } else {
                "✅ НОРМА (Кавитации нет)"
            }
        );
        println!("--------------------------------------------------------------");
    }
}
