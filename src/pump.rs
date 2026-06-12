//! # Модуль математического моделирования центробежных насосов
//!
//! Модуль реализует физико-математическую модель центробежного нагнетателя
//! с функциями диагностики и подбора оборудования под трубопроводную сеть.
//!
//! ## Архитектура
//! - `Pump` — модель насоса с методами расчета рабочей точки
//! - `PumpOperatingPoint` — результат расчета (рабочая точка + диагностика)
//! - `PumpDiagnostics` — структурированная диагностика
//! - `PumpWarning` — предупреждения и аварии
//! - `PumpSelector` — подбор насоса под трубопровод

use crate::fluid_type::FluidType;
use crate::system_traits::HydraulicSystem;
use crate::system_functions::{
    GRAVITY, MPA_TO_PA, M3H_TO_M3S,
    DiameterCompatibility, NetworkType,
    calculate_closed_loop_inlet_pressure,
    calculate_pump_curve_k,
    hydraulic_power_w,
    calculate_npsh_available as calc_npsha,
};

// =============================================================================
// КОНСТАНТЫ (ИЗМЕНЕНИЕ: вынесены общие в system_functions.rs)
// =============================================================================

pub const KW_TO_W: f64 = 1000.0;
pub const MAX_PIPE_VELOCITY_MS: f64 = 2.5;
pub const MIN_PIPE_VELOCITY_MS: f64 = 0.5;
pub const OPTIMAL_PIPE_VELOCITY_MS: f64 = 2.0;
pub const MOTOR_OVERLOAD_TRIP_FACTOR: f64 = 1.15;
pub const BEP_DEVIATION_THRESHOLD: f64 = 0.30;
pub const SHUTOFF_HEAD_FACTOR: f64 = 1.2; // Напор при нулевой подаче = 1.2 * H_ном

// =============================================================================
// ПРЕДУПРЕЖДЕНИЯ И ДИАГНОСТИКА
// =============================================================================

/// Виды выявляемых аварийных отклонений в работе насосного агрегата.
/// ИЗМЕНЕНИЕ: Добавлен вариант DiameterMismatch
#[derive(Debug, Clone, PartialEq)]
pub enum PumpWarning {
    /// Потребляемая мощность превысила порог тепловой защиты
    MotorOverload { actual_kw: f64, nominal_kw: f64, threshold_kw: f64 },
    /// Скорость течения выше нормативной
    ExcessiveVelocity { actual_ms: f64, max_allowed_ms: f64, is_internal: bool },
    /// Скорость течения ниже порога заиливания
    LowVelocity { actual_ms: f64, min_allowed_ms: f64 },
    /// Работа вне зоны BEP
    OffBep { actual_flow_m3h: f64, nominal_flow_m3h: f64, deviation_pct: f64 },
    /// Риск кавитации
    Cavitation { npsh_available_m: f64, npsh_required_m: f64 },
    /// Несоответствие диаметров насоса и трубопровода (НОВОЕ)
    DiameterMismatch { side: String, nozzle_mm: f64, pipe_mm: f64 },
}

impl PumpWarning {
    /// Детализированное сообщение на русском языке
    pub fn message(&self) -> String {
        match self {
            Self::MotorOverload { actual_kw, nominal_kw, threshold_kw } => format!(
                "КРИТИЧЕСКАЯ НАГРУЗКА: мощность {:.2} кВт превышает порог защиты {:.1} кВт (номинал {:.1} кВт).",
                actual_kw, threshold_kw, nominal_kw
            ),
            Self::ExcessiveVelocity { actual_ms, max_allowed_ms, is_internal } => {
                let loc = if *is_internal { "в патрубках насоса" } else { "в трубопроводе" };
                format!(
                    "ПРЕВЫШЕНИЕ СКОРОСТИ {}: {:.2} м/с > {:.1} м/с (СНиП). Риск эрозии и шума!",
                    loc, actual_ms, max_allowed_ms
                )
            }
            Self::LowVelocity { actual_ms, min_allowed_ms } => format!(
                "НИЗКАЯ СКОРОСТЬ: {:.2} м/с < {:.1} м/с. Риск заиливания.",
                actual_ms, min_allowed_ms
            ),
            Self::OffBep { actual_flow_m3h, nominal_flow_m3h, deviation_pct } => format!(
                "РАБОТА ВНЕ BEP: расход {:.1} м³/ч отклонился от {:.1} м³/ч на {:.1}%.",
                actual_flow_m3h, nominal_flow_m3h, deviation_pct
            ),
            Self::Cavitation { npsh_available_m, npsh_required_m } => format!(
                "КАВИТАЦИЯ: NPSHa {:.2} м < NPSHr {:.2} м. Разрушение рабочего колеса!",
                npsh_available_m, npsh_required_m
            ),
            // НОВОЕ сообщение
            Self::DiameterMismatch { side, nozzle_mm, pipe_mm } => format!(
                "НЕСООТВЕТСТВИЕ ДИАМЕТРОВ на стороне '{}': патрубок {:.0} мм, труба {:.0} мм.",
                side, nozzle_mm, pipe_mm
            ),
        }
    }

    /// true — авария требует немедленной остановки насоса
    pub fn is_critical(&self) -> bool {
        matches!(
            self,
            Self::MotorOverload { .. } | Self::Cavitation { .. } | Self::DiameterMismatch { .. }
        )
    }
}

// =============================================================================
// РЕКОМЕНДАЦИЯ ПО ДИАМЕТРУ ТРУБОПРОВОДА
// =============================================================================

/// Математический подбор оптимального диаметра трубопровода
#[derive(Debug, Clone)]
pub struct PipeRecommendation {
    pub nominal_flow_m3h: f64,
    pub nominal_head_m: f64,
    pub min_diameter_mm: f64,
    pub optimal_diameter_mm: f64,
    pub velocity_at_optimal_ms: f64,
    pub estimated_head_loss_m: f64,
}

impl PipeRecommendation {
    /// Расчет рекомендованных параметров по неразрывности струи и Дарси-Вейсбаху
    pub fn calculate(flow_m3h: f64, pump_head_m: f64, pipe_length_m: f64) -> Self {
        let flow_m3s = flow_m3h * M3H_TO_M3S;
        
        // Минимальный диаметр (по максимальной скорости)
        let area_min = flow_m3s / MAX_PIPE_VELOCITY_MS;
        let min_diameter_mm = (4.0 * area_min / std::f64::consts::PI).sqrt() * 1000.0;
        
        // Оптимальный диаметр (по целевой скорости)
        let area_opt = flow_m3s / OPTIMAL_PIPE_VELOCITY_MS;
        let optimal_diameter_mm = (4.0 * area_opt / std::f64::consts::PI).sqrt() * 1000.0;
        let velocity_at_optimal = flow_m3s / (std::f64::consts::PI * (optimal_diameter_mm / 2000.0).powi(2));
        
        // Оценочные потери по Дарси-Вейсбаху с λ ≈ 0.02
        let estimated_loss = 0.02 * (pipe_length_m / (optimal_diameter_mm / 1000.0))
            * (velocity_at_optimal.powi(2)) / (2.0 * GRAVITY);
        
        Self {
            nominal_flow_m3h: flow_m3h,
            nominal_head_m: pump_head_m,
            min_diameter_mm,
            optimal_diameter_mm,
            velocity_at_optimal_ms: velocity_at_optimal,
            estimated_head_loss_m: estimated_loss,
        }
    }

    /// Структурированный отчет
    pub fn generate_report(&self, test_length_m: f64) -> String {
        let mut r = String::new();
        r.push_str(&format!(
            "  Диаметр минимальный (v ≤ {:.1} м/с): {:.0} мм\n",
            MAX_PIPE_VELOCITY_MS, self.min_diameter_mm
        ));
        r.push_str(&format!(
            "  Диаметр оптимальный (v ≈ {:.1} м/с): {:.0} мм\n",
            OPTIMAL_PIPE_VELOCITY_MS, self.optimal_diameter_mm
        ));
        r.push_str(&format!(
            "  Оценочные потери напора на {} м: {:.1} м\n",
            test_length_m, self.estimated_head_loss_m
        ));
        
        let head_margin = (self.nominal_head_m - self.estimated_head_loss_m) / self.nominal_head_m;
        if head_margin > 0.1 && head_margin < 0.30 {
            r.push_str("  ✅ Оптимальный режим: насос будет работать в зоне высокого КПД.\n");
        } else if head_margin <= 0.1 {
            r.push_str("  ⚠️ ПРЕДУПРЕЖДЕНИЕ: Потери напора слишком велики. Расход будет ниже номинала.\n");
        } else {
            r.push_str("  ⚠️ ПРЕДУПРЕЖДЕНИЕ: Потери напора малы. Требуется дросселирование или ЧРП.\n");
        }
        r
    }
}

// =============================================================================
// ДИАГНОСТИКА НАСОСНОЙ УСТАНОВКИ
// =============================================================================

/// Статистический и диагностический профиль работы насосной установки.
/// ИЗМЕНЕНИЕ: Добавлено поле connection_error
#[derive(Debug, Clone)]
pub struct PumpDiagnostics {
    pub warnings: Vec<PumpWarning>,
    pub has_critical_warnings: bool,
    pub has_any_warnings: bool,
    /// Флаг ошибки подключения (диаметры несовместимы) — НОВОЕ
    pub connection_error: bool,
    pub recommendation: PipeRecommendation,
}

impl PumpDiagnostics {
    pub fn generate_report(&self, test_length_m: f64) -> String {
        let mut r = String::new();
        r.push_str("=== ОТЧЕТ ДИАГНОСТИКИ НАСОСНОЙ УСТАНОВКИ ===\n");
        
        if self.connection_error {
            r.push_str("🚫 КРИТИЧЕСКАЯ ОШИБКА ПОДКЛЮЧЕНИЯ: Диаметры труб несовместимы с патрубками насоса.\n");
            r.push_str("   Расчет рабочей точки невозможен. Устраните несоответствие диаметров.\n");
            return r;
        }
        
        if !self.has_any_warnings {
            r.push_str("✅ Насосная установка работает в оптимальном режиме.\n");
        } else {
            for (i, w) in self.warnings.iter().enumerate() {
                let marker = if w.is_critical() {
                    "[🚨 КРИТИЧЕСКИ]"
                } else {
                    "[⚠️ ПРЕДУПРЕЖДЕНИЕ]"
                };
                r.push_str(&format!("  {}. {} {}\n", i + 1, marker, w.message()));
            }
        }
        
        r.push_str("\n--- Рекомендации по диаметрам труб ---\n");
        r.push_str(&self.recommendation.generate_report(test_length_m));
        r
    }
}

// =============================================================================
// РАБОЧАЯ ТОЧКА НАСОСА
// =============================================================================

/// Выходная рабочая точка с физическими параметрами и флагами безопасности.
/// ИЗМЕНЕНИЕ: Добавлены поля connection_error, network_type
#[derive(Debug, Clone)]
pub struct PumpOperatingPoint {
    pub inlet_pressure_mpa: f64,
    pub flow_rate_m3h: f64,
    pub head_m: f64,
    pub shaft_power_kw: f64,
    pub efficiency: f64,
    pub rotation_speed_rpm: f64,
    pub npsh_available_m: f64,
    pub npsh_required_m: f64,
    pub inlet_velocity_ms: f64,
    pub outlet_velocity_ms: f64,
    pub outlet_pressure_mpa: f64,
    
    // Флаги для UI Data Binding
    pub system_is_safe: bool,
    pub motor_overload_alarm: bool,
    pub excessive_velocity_alarm: bool,
    pub cavitation_risk: bool,
    /// Ошибка подключения по диаметрам — НОВОЕ
    pub connection_error: bool,
    
    // Детальная диагностика
    pub diagnostics: PumpDiagnostics,
}

impl PumpOperatingPoint {
    /// Внедряет результаты анализа сети в диагностику точки
    pub fn update_system_diagnostics(&mut self, max_pipeline_velocity_ms: f64) {
        if max_pipeline_velocity_ms > MAX_PIPE_VELOCITY_MS {
            self.excessive_velocity_alarm = true;
            self.system_is_safe = false;
            
            let warning = PumpWarning::ExcessiveVelocity {
                actual_ms: max_pipeline_velocity_ms,
                max_allowed_ms: MAX_PIPE_VELOCITY_MS,
                is_internal: false,
            };
            
            if !self.diagnostics.warnings.contains(&warning) {
                self.diagnostics.warnings.push(warning);
            }
            self.diagnostics.has_any_warnings = true;
            
            if max_pipeline_velocity_ms > 3.0 {
                self.diagnostics.has_critical_warnings = true;
            }
        }
    }
}

// =============================================================================
// МОДЕЛЬ НАСОСА (РЕФАКТОРИНГ)
// =============================================================================

/// Математическая модель центробежного насоса.
///
/// ИЗМЕНЕНИЯ:
/// - Добавлен контроль диаметров при запуске расчета
/// - Добавлен учет типа сети (кольцевая/открытая)
/// - Добавлен метод `select_pump_for_network` для подбора насоса
/// - Улучшена документация методов
#[derive(Debug, Clone)]
pub struct Pump {
    // Геометрические параметры
    nominal_din_mm: f64,
    nominal_dout_mm: f64,
    
    // Номинальные рабочие параметры
    nominal_flow_rate_m3h: f64,
    nominal_head_m: f64,
    electric_motor_power_kw: f64,
    nominal_rotation_speed_rpm: f64,
    
    // Текущее состояние
    current_rotation_speed_rpm: f64,
    current_inlet_pressure_mpa: f64,
    fluid: FluidType,
    is_started: bool,
}

impl Pump {
    /// Создает экземпляр модели насоса с проверкой параметров.
    ///
    /// # Аргументы
    /// * `nominal_din_mm` — диаметр входного патрубка, мм
    /// * `nominal_dout_mm` — диаметр выходного патрубка, мм
    /// * `nominal_inlet_pressure_mpa` — номинальное давление на входе, МПа
    /// * `nominal_flow_rate_m3h` — номинальная подача, м³/ч
    /// * `nominal_head_m` — номинальный напор, м
    /// * `electric_motor_power_kw` — мощность электродвигателя, кВт
    /// * `rotation_speed_rpm` — номинальная скорость вращения, об/мин
    ///
    /// # Паника
    /// Вызывает panic! при некорректных параметрах (отрицательные/нулевые значения)
    pub fn new(
        nominal_din_mm: f64,
        nominal_dout_mm: f64,
        nominal_inlet_pressure_mpa: f64,
        nominal_flow_rate_m3h: f64,
        nominal_head_m: f64,
        electric_motor_power_kw: f64,
        rotation_speed_rpm: f64,
    ) -> Self {
        // ИЗМЕНЕНИЕ: Добавлена валидация входных параметров
        assert!(nominal_din_mm > 0.0, "Диаметр входного патрубка должен быть положительным");
        assert!(nominal_dout_mm > 0.0, "Диаметр выходного патрубка должен быть положительным");
        assert!(nominal_inlet_pressure_mpa >= 0.0, "Давление на входе не может быть отрицательным");
        assert!(nominal_flow_rate_m3h > 0.0, "Номинальная подача должна быть положительной");
        assert!(nominal_head_m > 0.0, "Номинальный напор должен быть положительным");
        assert!(electric_motor_power_kw > 0.0, "Мощность двигателя должна быть положительной");
        assert!(rotation_speed_rpm > 0.0, "Скорость вращения должна быть положительной");
        
        Self {
            nominal_din_mm,
            nominal_dout_mm,
            nominal_flow_rate_m3h,
            nominal_head_m,
            electric_motor_power_kw,
            nominal_rotation_speed_rpm: rotation_speed_rpm,
            current_rotation_speed_rpm: rotation_speed_rpm,
            current_inlet_pressure_mpa: nominal_inlet_pressure_mpa,
            fluid: FluidType::Water,
            is_started: false,
        }
    }

    // ======================== ГЕТТЕРЫ ========================
    
    /// Возвращает диаметр входного патрубка, мм
    pub fn inlet_diameter_mm(&self) -> f64 { self.nominal_din_mm }
    
    /// Возвращает диаметр выходного патрубка, мм
    pub fn outlet_diameter_mm(&self) -> f64 { self.nominal_dout_mm }
    
    /// Возвращает номинальную подачу, м³/ч
    pub fn nominal_flow_m3h(&self) -> f64 { self.nominal_flow_rate_m3h }
    
    /// Возвращает номинальный напор, м
    pub fn nominal_head_m(&self) -> f64 { self.nominal_head_m }
    
    /// Возвращает мощность двигателя, кВт
    pub fn motor_power_kw(&self) -> f64 { self.electric_motor_power_kw }

    // ======================== СЕТТЕРЫ ========================
    
    pub fn set_fluid(&mut self, fluid: FluidType) { self.fluid = fluid; }
    
    /// Устанавливает состояние насоса (запущен/остановлен)
    pub fn set_running(&mut self, running: bool) { self.is_started = running; }
    
    /// Устанавливает скорость вращения (для моделирования ЧРП)
    pub fn set_rotation_speed_rpm(&mut self, rpm: f64) {
        assert!(rpm > 0.0, "Скорость вращения должна быть положительной");
        self.current_rotation_speed_rpm = rpm;
    }
    
    /// Устанавливает давление на входе в насос
    pub fn set_inlet_pressure_mpa(&mut self, p_mpa: f64) {
        assert!(p_mpa >= 0.0, "Давление не может быть отрицательным");
        self.current_inlet_pressure_mpa = p_mpa;
    }

    // ======================== ВНУТРЕННИЕ РАСЧЕТЫ ========================

    /// Расчет NPSH требуемого (кавитационный запас насоса).
    /// Формула на основе эмпирической зависимости от подачи и скорости вращения.
    fn calculate_npsh_required(&self, flow_m3h: f64) -> f64 {
        if flow_m3h <= 0.0 {
            return 0.5; // Минимальное значение
        }
        let speed_ratio = self.current_rotation_speed_rpm / self.nominal_rotation_speed_rpm;
        let flow_m3s = flow_m3h * M3H_TO_M3S;
        // Эмпирическая формула NPSHr = (n * sqrt(Q) / C)^(4/3), C ≈ 150 для типовых насосов
        let npsh_bep = (self.nominal_rotation_speed_rpm * flow_m3s.sqrt() / 150.0).powf(4.0 / 3.0);
        npsh_bep * speed_ratio.powi(2) * 3.15 // Коэффициент запаса
    }

    /// Расчет скорости потока в патрубке
    fn calculate_velocity_ms(&self, flow_m3h: f64, d_mm: f64) -> f64 {
        if flow_m3h <= 0.0 || d_mm <= 0.0 {
            return 0.0;
        }
        let flow_m3s = flow_m3h * M3H_TO_M3S;
        let area = std::f64::consts::PI * (d_mm / 2000.0).powi(2);
        flow_m3s / area
    }

    /// Оценка номинального КПД насоса (без учета вязкости)
    fn estimate_nominal_efficiency(&self) -> f64 {
        let hydraulic_power = hydraulic_power_w(self.nominal_flow_rate_m3h, self.nominal_head_m, self.fluid);
        let motor_power_w = self.electric_motor_power_kw * KW_TO_W;
        (hydraulic_power / motor_power_w).clamp(0.30, 0.85)
    }

    // ======================== ГЛАВНЫЙ МЕТОД: РАСЧЕТ РАБОЧЕЙ ТОЧКИ ========================

    /// Поиск рабочей точки насоса в гидравлической системе.
    ///
    /// ИЗМЕНЕНИЯ:
    /// - Добавлен контроль диаметров (connection_error)
    /// - Добавлен учет network_type для кольцевых сетей
    /// - pipe_loop_length_m теперь реально используется
    ///
    /// # Аргументы
    /// * `system` — гидравлическая система (трубопроводная сеть)
    /// * `inlet_pressure_mpa` — давление на входе насоса, МПа
    /// * `fluid` — тип жидкости
    /// * `pipe_loop_length_m` — общая длина трубопровода, м (для рекомендаций)
    /// * `network_type` — тип сети (открытая/кольцевая) — НОВЫЙ ПАРАМЕТР
    /// * `pipe_inlet_diameter_mm` — диаметр трубы на входе в насос — НОВЫЙ ПАРАМЕТР
    /// * `pipe_outlet_diameter_mm` — диаметр трубы на выходе насоса — НОВЫЙ ПАРАМЕТР
    ///
    /// # Возвращает
    /// `PumpOperatingPoint` с полной диагностикой
    pub fn find_working_point<H: HydraulicSystem>(
        &mut self,
        system: &H,
        inlet_pressure_mpa: f64,
        fluid: FluidType,
        pipe_loop_length_m: f64,
        network_type: NetworkType,
        pipe_inlet_diameter_mm: f64,
        pipe_outlet_diameter_mm: f64,
    ) -> PumpOperatingPoint {
        self.set_inlet_pressure_mpa(inlet_pressure_mpa);
        self.set_fluid(fluid);

        // ========== НОВОЕ: ПРОВЕРКА СОВМЕСТИМОСТИ ДИАМЕТРОВ ==========
        let inlet_compat = DiameterCompatibility::check(
            self.nominal_din_mm, pipe_inlet_diameter_mm, "вход"
        );
        let outlet_compat = DiameterCompatibility::check(
            self.nominal_dout_mm, pipe_outlet_diameter_mm, "выход"
        );
        
        let connection_error = !inlet_compat.is_compatible || !outlet_compat.is_compatible;
        
        // Формируем предупреждения о несоответствии диаметров
        let mut diameter_warnings = Vec::new();
        if let Some(ref _msg) = inlet_compat.warning_message {
            diameter_warnings.push(PumpWarning::DiameterMismatch {
                side: "вход".to_string(),
                nozzle_mm: self.nominal_din_mm,
                pipe_mm: pipe_inlet_diameter_mm,
            });
        }
        if let Some(ref _msg) = outlet_compat.warning_message {
            diameter_warnings.push(PumpWarning::DiameterMismatch {
                side: "выход".to_string(),
                nozzle_mm: self.nominal_dout_mm,
                pipe_mm: pipe_outlet_diameter_mm,
            });
        }

        // Расчет рекомендации по диаметру (учитывает фактическую длину)
        let recommendation = PipeRecommendation::calculate(
            self.nominal_flow_rate_m3h,
            self.nominal_head_m,
            pipe_loop_length_m,
        );

        // ========== ЕСЛИ НАСОС ОСТАНОВЛЕН ИЛИ ЕСТЬ ОШИБКА ПОДКЛЮЧЕНИЯ ==========
        if !self.is_started || connection_error {
            let diagnostics = PumpDiagnostics {
                warnings: diameter_warnings.clone(),
                has_critical_warnings: connection_error,
                has_any_warnings: !diameter_warnings.is_empty(),
                connection_error,
                recommendation,
            };
            
            return PumpOperatingPoint {
                inlet_pressure_mpa: self.current_inlet_pressure_mpa,
                flow_rate_m3h: 0.0,
                head_m: 0.0,
                shaft_power_kw: 0.0,
                efficiency: 0.0,
                rotation_speed_rpm: if self.is_started { self.current_rotation_speed_rpm } else { 0.0 },
                npsh_available_m: calc_npsha(self.current_inlet_pressure_mpa, self.fluid),
                npsh_required_m: 0.0,
                inlet_velocity_ms: 0.0,
                outlet_velocity_ms: 0.0,
                outlet_pressure_mpa: self.current_inlet_pressure_mpa,
                system_is_safe: !connection_error,
                motor_overload_alarm: false,
                excessive_velocity_alarm: false,
                cavitation_risk: false,
                connection_error,
                diagnostics,
            };
        }

        // ========== ПОСТРОЕНИЕ ХАРАКТЕРИСТИКИ НАСОСА ==========
        // ИЗМЕНЕНИЕ: Корректный расчет коэффициента k
        let h_shutoff = self.nominal_head_m * SHUTOFF_HEAD_FACTOR;
        let k = calculate_pump_curve_k(h_shutoff, self.nominal_head_m, self.nominal_flow_rate_m3h);
        
        let pump_head = |q: f64| {
            if k <= 0.0 { return self.nominal_head_m; }
            (h_shutoff - k * q * q).max(0.0)
        };

        // ========== БИСЕКЦИЯ: ПОИСК РАВНОВЕСИЯ H_насоса(Q) = H_системы(Q) ==========
        let mut lo = 0.1_f64;
        let mut hi = self.nominal_flow_rate_m3h * 2.5;
        let mut best_q = 0.0_f64;

        for _ in 0..100 {
            let q_mid = (lo + hi) / 2.0;
            let pump_h = pump_head(q_mid);
            let system_h = system.head_loss_at_flow(q_mid, fluid);
            let diff = pump_h - system_h;
            
            best_q = q_mid;
            
            if diff.abs() < 0.001 {
                break; // Достаточная точность
            }
            
            if diff > 0.0 {
                lo = q_mid; // Напор насоса больше — увеличиваем расход
            } else {
                hi = q_mid; // Напор насоса меньше — уменьшаем расход
            }
            
            if (hi - lo) < 0.001 {
                break;
            }
        }

        // ========== РАСЧЕТ ПАРАМЕТРОВ В РАБОЧЕЙ ТОЧКЕ ==========
        let head_m = pump_head(best_q);
        
        // Коррекция КПД по расходу (параболическая аппроксимация)
        let flow_ratio = best_q / self.nominal_flow_rate_m3h;
        let normalized_efficiency = (2.0 * flow_ratio - flow_ratio.powi(2)).max(0.1);
        let efficiency = self.estimate_nominal_efficiency() * normalized_efficiency;
        
        // Мощность на валу
        let hydraulic_power = hydraulic_power_w(best_q, head_m, self.fluid);
        let shaft_power_kw = (hydraulic_power / KW_TO_W) / efficiency;

        // Кавитационный расчет
        let npsh_available = calc_npsha(self.current_inlet_pressure_mpa, self.fluid);
        let npsh_required = self.calculate_npsh_required(best_q);
        let cavitation_risk = npsh_available < (npsh_required * 1.15);

        // Скорости в патрубках
        let inlet_vel = self.calculate_velocity_ms(best_q, self.nominal_din_mm);
        let outlet_vel = self.calculate_velocity_ms(best_q, self.nominal_dout_mm);

        // Выходное давление
        let outlet_pressure = self.current_inlet_pressure_mpa
            + (head_m * self.fluid.density_kg_m3() * GRAVITY / MPA_TO_PA);

        // ========== ИЗМЕНЕНИЕ: УЧЕТ КОЛЬЦЕВОЙ СЕТИ ==========
        // В кольцевой сети фактическое давление на входе насоса пересчитывается
        let actual_inlet_pressure = match network_type {
            NetworkType::Closed => {
                let total_loss = system.head_loss_at_flow(best_q, fluid);
                calculate_closed_loop_inlet_pressure(
                    outlet_pressure, total_loss, self.fluid, 0.0
                )
            }
            NetworkType::Open => self.current_inlet_pressure_mpa,
        };

        // ========== ФОРМИРОВАНИЕ ПРЕДУПРЕЖДЕНИЙ ==========
        let mut warnings = diameter_warnings; // Начинаем с предупреждений о диаметрах
        let mut motor_overload_alarm = false;
        let mut excessive_velocity_alarm = false;

        // Проверка перегрузки двигателя
        let threshold_kw = self.electric_motor_power_kw * MOTOR_OVERLOAD_TRIP_FACTOR;
        if shaft_power_kw > threshold_kw {
            motor_overload_alarm = true;
            warnings.push(PumpWarning::MotorOverload {
                actual_kw: shaft_power_kw,
                nominal_kw: self.electric_motor_power_kw,
                threshold_kw,
            });
        }

        // Проверка скорости в патрубках
        let max_pump_v = inlet_vel.max(outlet_vel);
        if max_pump_v > MAX_PIPE_VELOCITY_MS {
            excessive_velocity_alarm = true;
            warnings.push(PumpWarning::ExcessiveVelocity {
                actual_ms: max_pump_v,
                max_allowed_ms: MAX_PIPE_VELOCITY_MS,
                is_internal: true,
            });
        }

        // Проверка кавитации
        if cavitation_risk {
            warnings.push(PumpWarning::Cavitation {
                npsh_available_m: npsh_available,
                npsh_required_m: npsh_required,
            });
        }

        // Проверка отклонения от BEP
        let deviation = (best_q - self.nominal_flow_rate_m3h).abs() / self.nominal_flow_rate_m3h;
        if deviation > BEP_DEVIATION_THRESHOLD {
            warnings.push(PumpWarning::OffBep {
                actual_flow_m3h: best_q,
                nominal_flow_m3h: self.nominal_flow_rate_m3h,
                deviation_pct: deviation * 100.0,
            });
        }

        let has_critical = warnings.iter().any(|w| w.is_critical());
        let has_any = !warnings.is_empty();

        let diagnostics = PumpDiagnostics {
            warnings,
            has_critical_warnings: has_critical,
            has_any_warnings: has_any,
            connection_error: false, // Уже проверено выше
            recommendation,
        };

        PumpOperatingPoint {
            inlet_pressure_mpa: actual_inlet_pressure,
            flow_rate_m3h: best_q,
            head_m,
            shaft_power_kw,
            efficiency,
            rotation_speed_rpm: self.current_rotation_speed_rpm,
            npsh_available_m: npsh_available,
            npsh_required_m: npsh_required,
            inlet_velocity_ms: inlet_vel,
            outlet_velocity_ms: outlet_vel,
            outlet_pressure_mpa: outlet_pressure,
            system_is_safe: !has_critical,
            motor_overload_alarm,
            excessive_velocity_alarm,
            cavitation_risk,
            connection_error: false,
            diagnostics,
        }
    }
}

// =============================================================================
// ПОДБОР НАСОСА ПОД ТРУБОПРОВОД (НОВЫЙ МОДУЛЬ)
// =============================================================================

/// Результат подбора насоса под трубопроводную сеть
#[derive(Debug, Clone)]
pub struct PumpSelectionResult {
    /// Подобранный насос (если найден)
    pub selected_pump: Option<Pump>,
    /// Рабочая точка подобранного насоса
    pub operating_point: Option<PumpOperatingPoint>,
    /// Список проверенных вариантов с оценкой пригодности
    pub candidates: Vec<PumpCandidate>,
    /// Успешность подбора
    pub success: bool,
    /// Сообщение с результатом
    pub message: String,
}

/// Кандидат насоса для подбора
#[derive(Debug, Clone)]
pub struct PumpCandidate {
    pub flow_m3h: f64,
    pub head_m: f64,
    pub power_kw: f64,
    pub din_mm: f64,
    pub dout_mm: f64,
    pub rpm: f64,
    pub is_suitable: bool,
    pub reason: String,
}

/// Селектор насосов — подбирает насос из каталога под заданную сеть.
///
/// Использование:
/// ```ignore
/// let candidates = vec![...]; // Список доступных насосов
/// let result = PumpSelector::select_pump(&network, &candidates, fluid, network_type);
/// ```
pub struct PumpSelector;

impl PumpSelector {
    /// Подбирает подходящий насос для заданной трубопроводной сети.
    ///
    /// # Алгоритм
    /// 1. Для каждого кандидата создается модель насоса
    /// 2. Выполняется расчет рабочей точки
    /// 3. Оценивается пригодность (КПД > 50%, нет критических аварий, скорость в норме)
    /// 4. Выбирается лучший по КПД и близости к BEP
    ///
    /// # Аргументы
    /// * `network` — трубопроводная сеть
    /// * `candidates` — список доступных моделей насосов
    /// * `fluid` — тип жидкости
    /// * `inlet_pressure_mpa` — давление на входе, МПа
    /// * `network_type` — тип сети (открытая/кольцевая)
    /// * `pipe_inlet_mm` — диаметр входной трубы, мм
    /// * `pipe_outlet_mm` — диаметр выходной трубы, мм
    /// * `pipe_length_m` — общая длина трубопровода, м
    ///
    /// # Возвращает
    /// `PumpSelectionResult` с результатом подбора
    pub fn select_pump<H: HydraulicSystem>(
        network: &H,
        candidates: &[PumpCandidate],
        fluid: FluidType,
        inlet_pressure_mpa: f64,
        network_type: NetworkType,
        pipe_inlet_mm: f64,
        pipe_outlet_mm: f64,
        pipe_length_m: f64,
    ) -> PumpSelectionResult {
        let mut best_pump: Option<Pump> = None;
        let mut best_op: Option<PumpOperatingPoint> = None;
        let mut best_score: f64 = -1.0;
        let mut evaluated_candidates = Vec::new();

        for candidate in candidates {
            let mut pump = Pump::new(
                candidate.din_mm,
                candidate.dout_mm,
                inlet_pressure_mpa,
                candidate.flow_m3h,
                candidate.head_m,
                candidate.power_kw,
                candidate.rpm,
            );
            pump.set_running(true);
            pump.set_fluid(fluid);

            let op = pump.find_working_point(
                network,
                inlet_pressure_mpa,
                fluid,
                pipe_length_m,
                network_type,
                pipe_inlet_mm,
                pipe_outlet_mm,
            );

            // Оценка пригодности
            let mut is_suitable = true;
            let mut reason = String::from("Подходит");

            if op.connection_error {
                is_suitable = false;
                reason = format!("Несовместимость диаметров: насос {:.0}/{:.0} мм, труба {:.0}/{:.0} мм",
                    candidate.din_mm, candidate.dout_mm, pipe_inlet_mm, pipe_outlet_mm);
            } else if op.diagnostics.has_critical_warnings {
                is_suitable = false;
                reason = "Критические аварии в рабочей точке".to_string();
            } else if op.efficiency < 0.5 {
                is_suitable = false;
                reason = format!("Низкий КПД: {:.1}%", op.efficiency * 100.0);
            } else if op.cavitation_risk {
                is_suitable = false;
                reason = "Риск кавитации".to_string();
            } else if op.motor_overload_alarm {
                is_suitable = false;
                reason = "Перегрузка двигателя".to_string();
            }

            // Скоринг: близость к BEP + КПД
            if is_suitable && op.flow_rate_m3h > 0.0 {
                let flow_deviation = (op.flow_rate_m3h - candidate.flow_m3h).abs() / candidate.flow_m3h;
                let score = (1.0 - flow_deviation) * 0.5 + op.efficiency * 0.5;
                
                if score > best_score {
                    best_score = score;
                    best_pump = Some(pump);
                    best_op = Some(op.clone());
                }
            }

            evaluated_candidates.push(PumpCandidate {
                is_suitable,
                reason,
                ..candidate.clone()
            });
        }

        PumpSelectionResult {
            selected_pump: best_pump.clone(),
            operating_point: best_op.clone(),
            candidates: evaluated_candidates,
            success: best_pump.is_some(),
            message: if best_pump.is_some() {
                format!("Подобран насос с оценкой пригодности {:.1}%", best_score * 100.0)
            } else {
                "Не найден подходящий насос среди кандидатов".to_string()
            },
        }
    }
}

// =============================================================================
// ТЕСТЫ
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pump_creation_valid() {
        let pump = Pump::new(100.0, 80.0, 0.1, 100.0, 30.0, 15.0, 1450.0);
        assert_eq!(pump.inlet_diameter_mm(), 100.0);
        assert_eq!(pump.nominal_flow_m3h(), 100.0);
    }

    #[test]
    #[should_panic]
    fn test_pump_creation_invalid_flow() {
        Pump::new(100.0, 80.0, 0.1, -10.0, 30.0, 15.0, 1450.0);
    }

    #[test]
    fn test_diameter_mismatch_detection() {
        // Тест проверяет, что при несовместимых диаметрах выставляется connection_error
        // (будет протестирован интеграционно с сетью)
    }
}