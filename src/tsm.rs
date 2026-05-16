
use rand::random_range;


mod constants {
    
    pub const ALPHA : f32 = 0.00428;
    pub const RHO_COPPER : f32 = 0.0175; // Ом * мм^2 / м

}

pub enum Class {
    ClassA,
    ClassB,
    ClassC,
}

pub enum NumbersOfWire {
    Wire2,
    Wire3,
    Wire4,
}





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



pub struct TSM {
    r0: f32,
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

    pub fn new(r0: f32, t_min: f32, t_max: f32, class: Class, num_of_wire: NumbersOfWire, wire_length: f32, wire_cross_section: f32, tau: f32) -> TSM {
        TSM {
            r0,
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
    
    fn temperature_to_resistance(&self) -> f32 {
        self.r0 * (1.0 + constants::ALPHA * self.sensor_temperature)
    }

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

    fn get_error(&self) -> f32 {
        let t = self.sensor_temperature.abs();
        match self.class {
            Class::ClassA => 0.15 + 0.002 * t,
            Class::ClassB => 0.3 + 0.005 * t,
            Class::ClassC => 0.6 + 0.01 * t,
        }
    }

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

    fn get_resistance(&self) -> f32 {
        let r_ideal = self.temperature_to_resistance();
        // Переводим погрешность из градусов в Омы ( sigma_R = delta_T * S )
        let sensitivity = self.r0 * constants::ALPHA;
        let sigma_r = self.get_error() * sensitivity;
        
        r_ideal + gauss_noise(sigma_r) + self.get_resistance_wires()
    }

    pub fn get_temperature(&self) -> f32 {
        (self.get_resistance() / self.r0 - 1.0) / constants::ALPHA
    }

    pub fn get_sensor_temp(&self) -> f32 {
        self.sensor_temperature
    }

    pub fn get_ambient_temp(&self) -> f32 {
        self.ambient_temperature
    }

    /// Возвращает ошибку (смещение), которую вносят провода в показания температуры (°C)
    pub fn get_wire_error_celsius(&self) -> f32 {
        let r_wires = self.get_resistance_wires();
        let sensitivity = self.r0 * constants::ALPHA;
        r_wires / sensitivity
    }
    
}