#![allow(non_camel_case_types)]
use rand::random_range;


mod constants {
    /// Удельное электрическое сопротивление меди: Ом * мм^2 / м
    pub const RHO_COPPER : f32 = 0.0175; 
}

/// Класс точности датчика
pub enum Class {
    ClassA,
    ClassB,
    ClassC,
}

/// Количество проводов
pub enum NumbersOfWire {
    Wire2,
    Wire3,
    Wire4,
}

/// Тип датчика
pub enum TypeSensor {
    Type50M,
    Type100M_426,
    Type100M_428,
}





/// Генерация случайного числа по Гауссу для имитации погрешностей измерения
/// шума датчика
fn gauss_noise(sigma : f32) -> f32 {
    let mut u1: f32 = random_range(0.0..=1.0);
    // Защита от логарифма нуля: u1 должен быть в диапазоне (0, 1]
    while u1 <= 0.0 {
        u1 = random_range(0.0..=1.0);
    }
    let u2: f32 = random_range(0.0..=1.0);
    let z = f32::sqrt(-2.0 * f32::ln(u1)) * f32::cos(2.0 * std::f32::consts::PI * u2);
    sigma * z
}


/// Структура датчика температуры
pub struct TSM {
    sensor_type: TypeSensor,
    t_min: f32,
    t_max: f32,
    class: Class,
    num_of_wire: NumbersOfWire,
    wire_length: f32,         // Длина одного провода в метрах
    wire_cross_section: f32,  // Сечение провода в мм^2
    tau: f32,                // Постоянная времени инерции
    sensor_temperature: f32,  // Текущая температура самого датчика
    ambient_temperature: f32, // Температура окружающей среды
}

impl TSM {

    /// Создание нового датчика
    pub fn new(sensor_type: TypeSensor, t_min: f32, t_max: f32, class: Class, num_of_wire: NumbersOfWire, wire_length: f32, wire_cross_section: f32, tau: f32) -> TSM {
        TSM {
            sensor_type,
            t_min,
            t_max,
            class,
            num_of_wire,
            wire_length,
            wire_cross_section,
            tau,
            sensor_temperature: 0.0,
            ambient_temperature: 0.0,
        }
    }

    /// Получение температурного коэффициента
    fn calculate_alpha(&self) -> f32 {
        match self.sensor_type {
            TypeSensor::Type50M => 0.00428,
            TypeSensor::Type100M_426 => 0.00426,
            TypeSensor::Type100M_428 => 0.00428,
        }
    }

    /// Получение коэффициента B
    fn calculate_b(&self) -> f32 {
        match self.sensor_type {
            TypeSensor::Type50M | TypeSensor::Type100M_428 => -6.2032e-7,
            TypeSensor::Type100M_426 => 0.0,
        }
    }

    /// Получение коэффициента C
    fn calculate_c(&self) -> f32 {
        match self.sensor_type {
            TypeSensor::Type50M | TypeSensor::Type100M_428 => 8.5154e-10,
            TypeSensor::Type100M_426 => 0.0,
        }
    }

    /// Получение номинального сопротивления датчика
    fn get_r0(&self) -> f32 {
        match self.sensor_type {
            TypeSensor::Type50M => 50.0,
            TypeSensor::Type100M_426 => 100.0,
            TypeSensor::Type100M_428 => 100.0,
        }
    }


    /// Расчет идеального сопротивления по НСХ (ГОСТ) для заданной температуры (без учета погрешностей)
    fn calc_ideal_resistance(&self, t: f32) -> f32 {
        let r0 = self.get_r0();
        let a = self.calculate_alpha();
        
        if t >= 0.0 {
            r0 * (1.0 + a * t)
        } else {
            let b = self.calculate_b();
            let c = self.calculate_c();
            r0 * (1.0 + a * t + b * t * (t + 6.7) + c * t.powi(3))
        }
    }

    /// Преобразование текущей температуры датчика в идеальное сопротивление
    fn temperature_to_resistance(&self) -> f32 {
        self.calc_ideal_resistance(self.sensor_temperature)
    }
    // Установка температуры окружающей среды
    pub fn set_temperature_environment(&mut self, temperature: f32) {
        if temperature < self.t_min || temperature > self.t_max {
            panic!("Temperature {} is out of range [{}, {}]", temperature, self.t_min, self.t_max);
        }
        self.ambient_temperature = temperature;
    }

    /// Обновление состояния датчика (симуляция инерции)
    /// dt - шаг времени в секундах
    pub fn tick(&mut self, dt: f32) {
        // Формула апериодического звена: T_new = T_old + (dt/tau) * (T_env - T_old)
        let delta = (dt / self.tau) * (self.ambient_temperature - self.sensor_temperature);
        self.sensor_temperature += delta;
    }

    /// Получение погрешности датчика в градусах
    fn get_error(&self) -> f32 {
        let t = self.sensor_temperature.abs();
        match self.class {
            Class::ClassA => 0.15 + 0.002 * t,
            Class::ClassB => 0.3 + 0.005 * t,
            Class::ClassC => 0.6 + 0.01 * t,
        }
    }
    /// Получение сопротивления проводов
    fn get_resistance_wires(&self) -> f32 {
        let r_one_wire = self.calculate_wire_resistance();
        match self.num_of_wire {
            NumbersOfWire::Wire2 => r_one_wire * 2.0,
            NumbersOfWire::Wire3 => 0.0_f32, // В 3-х проводной схеме сопротивление компенсируется
            NumbersOfWire::Wire4 => 0.0_f32,
        }
    }

    /// Расчет сопротивления одного медного провода: R = rho * (L / S)
    fn calculate_wire_resistance(&self) -> f32 {
        constants::RHO_COPPER * (self.wire_length / self.wire_cross_section)
    }

    /// Получение сопротивления датчика с учетом погрешностей
    pub fn get_out_resistance_sensor(&self) -> f32 {
        let r_ideal = self.temperature_to_resistance();
        // Переводим погрешность из градусов в Омы ( sigma_R = delta_T * S )
        let sensitivity = self.get_r0() * self.calculate_alpha();
        let sigma_r = self.get_error() * sensitivity;
        
        r_ideal + gauss_noise(sigma_r) + self.get_resistance_wires()
    }
    
    /// Получение температуры из сопротивления (Эмуляция измерительного прибора/ПЛК)
    /// Использует метод бисекции для расчета температуры по "грязному" сопротивлению
    pub fn get_out_sensor_temperature(&self) -> f32 {
        let r_target = self.get_out_resistance_sensor();
        let r0 = self.get_r0();
        let a = self.calculate_alpha();

        if r_target >= r0 {
            // Линейный участок (t >= 0): точная обратная формула
            (r_target / r0 - 1.0) / a
        } else {
            // Нелинейный участок (t < 0): поиск корня методом половинного деления (бисекция)
            let mut left = -300.0; // Гарантированно ниже минимальной температуры
            let mut right = 0.0;   // Верхняя граница нелинейного участка
            
            // 50 итераций дадут колоссальную точность, достаточную для любого АЦП
            for _ in 0..50 {
                let mid = (left + right) / 2.0;
                let r_mid = self.calc_ideal_resistance(mid);
                
                if r_mid < r_target {
                    left = mid; // Искомая температура выше (ближе к нулю)
                } else {
                    right = mid; // Искомая температура ниже
                }
            }
            (left + right) / 2.0
        }
    }

    /// Получение температуры самого датчика
    pub fn get_real_sensor_temperature(&self) -> f32 {
        self.sensor_temperature
    }

    /// Получение температуры окружающей среды
    pub fn get_ambient_temperature(&self) -> f32 {
        self.ambient_temperature
    }

    /// Возвращает ошибку (смещение), которую вносят провода в показания температуры (°C)
    pub fn get_wire_error_celsius(&self) -> f32 {
        let r_wires = self.get_resistance_wires();
        let sensitivity = self.get_r0() * self.calculate_alpha();
        if sensitivity == 0.0 {
            0.0
        } else {
            r_wires / sensitivity
        }
    }
    
}