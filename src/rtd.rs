#![allow(non_camel_case_types)]
use rand::random_range;
use crate::types::{Class, TypeSensor};

/// Генерация случайного числа по Гауссу для имитации погрешностей измерения шума датчика
fn gauss_noise(sigma: f32) -> f32 {
    let mut u1: f32 = random_range(0.0..=1.0);
    // Защита от логарифма нуля: u1 должен быть в диапазоне (0, 1]
    while u1 <= 0.0 {
        u1 = random_range(0.0..=1.0);
    }
    let u2: f32 = random_range(0.0..=1.0);
    let z = f32::sqrt(-2.0 * f32::ln(u1)) * f32::cos(2.0 * std::f32::consts::PI * u2);
    sigma * z
}

/// Структура датчика температуры (Чистая физическая модель)
pub struct RTD {
    pub sensor_type: TypeSensor,
    pub t_min: f32,
    pub t_max: f32,
    pub class: Class,
    pub tau: f32,                // Постоянная времени инерции
    sensor_temperature: f32,      // Текущая температура самого датчика
    ambient_temperature: f32,     // Температура окружающей среды
    systematic_offset: f32,       // Индивидуальный заводской сдвиг датчика (от -1.0 до 1.0)
}

impl RTD {
    /// Создание нового датчика
    pub fn new(sensor_type: TypeSensor, t_min: f32, t_max: f32, class: Class, tau: f32) -> RTD {
        RTD {
            sensor_type,
            t_min,
            t_max,
            class,
            tau,
            sensor_temperature: 0.0,
            ambient_temperature: 0.0,
            // Случайный сдвиг характеристик конкретного датчика на заводе 
            // (например, он всегда завышает на 80% от допустимого класса)
            systematic_offset: random_range(-1.0..=1.0),
        }
    }

    /// Преобразование текущей температуры датчика в идеальное сопротивление
    fn temperature_to_resistance(&self) -> f32 {
        self.sensor_type.calc_ideal_resistance(self.sensor_temperature)
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

    /// Получение выходного сопротивления датчика (идеальное + заводская погрешность + динамический шум)
    pub fn get_out_resistance_sensor(&self) -> f32 {
        let r_ideal = self.temperature_to_resistance();
        
        // Чувствительность датчика (Ом / °C). Используем calculate_a, так как это средний наклон
        let sensitivity = self.sensor_type.get_r0() * self.sensor_type.calculate_a();
        
        // 1. Систематическая погрешность (Класс точности ГОСТ)
        // Это смещение постоянно для данного экземпляра датчика
        let max_error_celsius = self.get_error();
        let sys_error_r = max_error_celsius * sensitivity * self.systematic_offset;
        
        // 2. Динамический шум (наводки, тепловой шум)
        // В реальности он очень мал. Возьмем, например, 3-сигма равным 0.05 °C
        let dynamic_noise_celsius = 0.05;
        let sigma_noise_r = (dynamic_noise_celsius * sensitivity) / 3.0; 
        
        r_ideal + sys_error_r + gauss_noise(sigma_noise_r)
    }

    /// Получение температуры самого датчика (для отладки)
    pub fn get_real_sensor_temperature(&self) -> f32 {
        self.sensor_temperature
    }

    /// Получение температуры окружающей среды (для отладки)
    pub fn get_ambient_temperature(&self) -> f32 {
        self.ambient_temperature
    }
}
