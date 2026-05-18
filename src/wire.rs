pub const RHO_COPPER: f32 = 0.0175; // Ом * мм^2 / м

/// Количество проводов
#[derive(Clone, Copy)]
pub enum NumbersOfWire {
    Wire2,
    Wire3,
    Wire4,
}

/// Линия связи (кабель)
pub struct Wire {
    pub wire_length: f32,         // Длина одного провода в метрах
    pub wire_cross_section: f32,  // Сечение провода в мм^2
    pub num_of_wire: NumbersOfWire,
}

impl Wire {
    pub fn new(wire_length: f32, wire_cross_section: f32, num_of_wire: NumbersOfWire) -> Self {
        Wire {
            wire_length,
            wire_cross_section,
            num_of_wire,
        }
    }

    /// Расчет сопротивления одного медного провода: R = rho * (L / S)
    fn calculate_wire_resistance(&self) -> f32 {
        RHO_COPPER * (self.wire_length / self.wire_cross_section)
    }

    /// Получение сопротивления проводов, вносящего погрешность в измерительную цепь
    pub fn get_resistance_wires(&self) -> f32 {
        let r_one_wire = self.calculate_wire_resistance();
        match self.num_of_wire {
            NumbersOfWire::Wire2 => r_one_wire * 2.0,
            NumbersOfWire::Wire3 => 0.0, // Компенсируется схемой ПЛК
            NumbersOfWire::Wire4 => 0.0, // Компенсируется схемой ПЛК
        }
    }

    /// Пропускает сопротивление датчика через линию, добавляя сопротивление проводов
    pub fn transmit(&self, r_sensor: f32) -> f32 {
        r_sensor + self.get_resistance_wires()
    }
}
