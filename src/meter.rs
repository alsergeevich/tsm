use crate::types::TypeSensor;

/// Измерительный прибор (ПЛК / АЦП)
pub struct Meter {
    pub sensor_type: TypeSensor,
}

impl Meter {
    pub fn new(sensor_type: TypeSensor) -> Self {
        Meter { sensor_type }
    }

    /// Измерение температуры по входящему сырому сопротивлению (Эмуляция работы ПЛК)
    pub fn measure(&self, r_input: f32) -> f32 {
        // Используем универсальный метод бисекции для всего диапазона температур,
        // так как кривая Pt100 нелинейна (квадратична) даже при t > 0.
        let mut left = -250.0; // Гарантированно ниже минимальной температуры
        let mut right = 1100.0;  // Верхняя граница с запасом
        
        // 50 итераций дадут точность лучше 10^-10 градуса
        for _ in 0..50 {
            let mid = (left + right) / 2.0;
            let r_mid = self.sensor_type.calc_ideal_resistance(mid);
            
            if r_mid < r_input {
                left = mid; // Искомая температура выше
            } else {
                right = mid; // Искомая температура ниже
            }
        }
        (left + right) / 2.0
    }
}
