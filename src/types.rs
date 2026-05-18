#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
/// Класс точности датчика
#[derive(Clone, Copy)]
pub enum Class {
    ClassA,
    ClassB,
    ClassC,
}

/// Тип датчика
#[derive(Clone, Copy)]
pub enum TypeSensor {
    Type50M,
    Type100M_426,
    Type100M_428,
    TypePt100,
    TypePt500,
    TypePt1000,
}

impl TypeSensor {
    /// Получение температурного коэффициента
    pub fn calculate_alpha(&self) -> f32 {
        match self {
            TypeSensor::Type50M => 0.00428,
            TypeSensor::Type100M_426 => 0.00426,
            TypeSensor::Type100M_428 => 0.00428,
            TypeSensor::TypePt100 => 0.00385,
            TypeSensor::TypePt500 => 0.00385,
            TypeSensor::TypePt1000 => 0.00385,
        }
    }
    pub fn calculate_a(&self) -> f32 {
        match self {
            TypeSensor::Type50M => 4.28e-3,
            TypeSensor::Type100M_426 => 4.26e-3,
            TypeSensor::Type100M_428 => 4.28e-3,
            TypeSensor::TypePt100 => 3.9083e-3,
            TypeSensor::TypePt500 => 3.9083e-3,
            TypeSensor::TypePt1000 => 3.9083e-3,
        }
    }
    /// Получение коэффициента B
    pub fn calculate_b(&self) -> f32 {
        match self {
            TypeSensor::Type50M | TypeSensor::Type100M_428 => -6.2032e-7,
            TypeSensor::Type100M_426 => 0.0,
            TypeSensor::TypePt100 | TypeSensor::TypePt500 | TypeSensor::TypePt1000 => -5.775e-7,
        }
    }

    /// Получение коэффициента C
    pub fn calculate_c(&self) -> f32 {
        match self {
            TypeSensor::Type50M | TypeSensor::Type100M_428 => 8.5154e-10,
            TypeSensor::Type100M_426 => 0.0,
            TypeSensor::TypePt100 | TypeSensor::TypePt500 | TypeSensor::TypePt1000 => -4.183e-12,
        }
    }

    /// Получение номинального сопротивления датчика
    pub fn get_r0(&self) -> f32 {
        match self {
            TypeSensor::Type50M => 50.0,
            TypeSensor::Type100M_426 => 100.0,
            TypeSensor::Type100M_428 => 100.0,
            TypeSensor::TypePt100 => 100.0,
            TypeSensor::TypePt500 => 500.0,
            TypeSensor::TypePt1000 => 1000.0,
        }
    }

    /// Расчет идеального сопротивления по НСХ (ГОСТ) для заданной температуры (без учета погрешностей)
    pub fn calc_ideal_resistance(&self, t: f32) -> f32 {
        match self {
            TypeSensor::Type50M | TypeSensor::Type100M_426 | TypeSensor::Type100M_428 => self.calc_ideal_resistance_M(t),
            TypeSensor::TypePt100 | TypeSensor::TypePt500 | TypeSensor::TypePt1000 => self.calc_ideal_resistance_Pt(t),
        }
    }

    fn calc_ideal_resistance_M(&self, t: f32) -> f32 {
        let r0 = self.get_r0();
        let a = self.calculate_a();
        
        if t >= 0.0 {
            r0 * (1.0 + a * t)
        } else {
            let b = self.calculate_b();
            let c = self.calculate_c();
            r0 * (1.0 + a * t + b * t * (t + 6.7) + c * t.powi(3))
        }
    }

    fn calc_ideal_resistance_Pt(&self, t: f32) -> f32 {
        let r0 = self.get_r0();
        let a = self.calculate_a();
        let b = self.calculate_b();
        let c = self.calculate_c();
        if t >= 0.0 {
            r0 * (1.0 + a * t + b * t.powi(2))
        } else {
            r0 * (1.0 + a * t + b * t.powi(2) + c * (t - 100.0) * t.powi(3))
        }
    }
}
