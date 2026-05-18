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
        let r0 = self.sensor_type.get_r0();
        let a = self.sensor_type.calculate_alpha();

        if r_input >= r0 {
            // Линейный участок (t >= 0): точная обратная формула
            (r_input / r0 - 1.0) / a
        } else {
            // Нелинейный участок (t < 0): поиск корня методом половинного деления (бисекция)
            let mut left = -300.0; // Гарантированно ниже минимальной температуры
            let mut right = 0.0;   // Верхняя граница нелинейного участка
            
            // 50 итераций дадут колоссальную точность
            for _ in 0..50 {
                let mid = (left + right) / 2.0;
                let r_mid = self.sensor_type.calc_ideal_resistance(mid);
                
                if r_mid < r_input {
                    left = mid; // Искомая температура выше (ближе к нулю)
                } else {
                    right = mid; // Искомая температура ниже
                }
            }
            (left + right) / 2.0
        }
    }
}
