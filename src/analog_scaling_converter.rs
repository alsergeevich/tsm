#![allow(unused)]
#![allow(non_snake_case)]

/// Универсальный блок аналогового масштабирования.
/// 
/// Преобразует входной сигнал из одного диапазона в другой по линейной формуле.
/// Не зависит от физического смысла сигнала (может масштабировать Омы в градусы, градусы в мА и т.д.).
pub struct AnalogScaling {
    value_input: f32,
    input_Min: f32,
    input_Max: f32,
    value_output: f32,
    output_Min: f32,
    output_Max: f32,
    error: bool,
}

impl AnalogScaling {

    /// Создание нового блока масштабирования.
    /// 
    /// # Аргументы
    /// * `input_Min` — нижняя граница входного диапазона
    /// * `input_Max` — верхняя граница входного диапазона
    /// * `output_Min` — нижняя граница выходного диапазона
    /// * `output_Max` — верхняя граница выходного диапазона
    pub fn new(
        input_Min: f32,
        input_Max: f32,
        output_Min: f32,
        output_Max: f32,
    ) -> Self {
        let error = input_Min >= input_Max || output_Min >= output_Max;
        Self {
            value_input: 0.0,
            input_Min,
            input_Max,
            value_output: 0.0,
            output_Min,
            output_Max,
            error,
        }
    }

    /// Внутренний метод масштабирования: чистая математика без проверок.
    fn analog_scaling(&mut self) -> f32 {
        (self.value_input - self.input_Min) / (self.input_Max - self.input_Min)
            * (self.output_Max - self.output_Min)
            + self.output_Min
    }

    /// Внутренний метод пересчета: вызывается автоматически при get_value_output.
    fn update(&mut self) {
        if self.error {
            self.value_output = 0.0;
            return;
        }
        self.value_output = self.analog_scaling();
    }

    /// Устанавливает входное значение
    pub fn set_value_input(&mut self, value_input: f32) {
        self.value_input = value_input;
    }

    /// Возвращает вычисленное выходное значение. Пересчёт выполняется автоматически.
    pub fn get_value_output(&mut self) -> f32 {
        self.update();
        self.value_output
    }

    /// Возвращает флаг ошибки. `true` — диапазоны некорректны.
    pub fn get_error(&self) -> bool {
        self.error
    }

    /// Задаёт новый диапазон входного сигнала.
    pub fn set_input_range(&mut self, input_Min: f32, input_Max: f32) {
        self.input_Min = input_Min;
        self.input_Max = input_Max;
        // Проверяем ОБА диапазона
        self.error = self.input_Min >= self.input_Max || self.output_Min >= self.output_Max;
    }

    /// Задаёт новый диапазон выходного сигнала.
    pub fn set_output_range(&mut self, output_Min: f32, output_Max: f32) {
        self.output_Min = output_Min;
        self.output_Max = output_Max;
        // Проверяем ОБА диапазона
        self.error = self.input_Min >= self.input_Max || self.output_Min >= self.output_Max;
    }
}