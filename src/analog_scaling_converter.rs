#![allow(unused)]
#![allow(non_snake_case)]
pub struct CurrentConverter {
    value_input : f32,
    input_Min : f32,
    input_Max : f32,
    value_output : f32,
    output_Min : f32,
    output_Max : f32,
    error : bool,
}

impl CurrentConverter {

    pub fn new(value_input : f32, input_Min : f32, input_Max : f32, output_Min : f32, output_Max : f32) -> Self {
        Self {
            value_input,
            input_Min,
            input_Max,
            value_output: 0.0,
            output_Min,
            output_Max,
            error:false
        }
    }


    fn analog_scaling(&mut self) -> f32 {
        (self.value_input - self.input_Min) / (self.input_Max - self.input_Min) * (self.output_Max - self.output_Min) + self.output_Min
    }


    /// Внутренний метод пересчета: вызывается автоматически при get_value_output.
    fn update(&mut self) {
        // Проверка корректности диапазонов
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

    /// Возвращает последнее вычисленное выходное значение (только чтение)
    pub fn get_value_output(&mut self) -> f32 {
        self.update();
        self.value_output
    }

    /// Возвращает флаг ошибки. true — диапазоны некорректны
    pub fn get_error(&self) -> bool {
        self.error
    }

    /// Задает новый диапазон входного сигнала
    pub fn set_input_range(&mut self, input_Min : f32, input_Max : f32) {
        self.input_Min = input_Min;
        self.input_Max = input_Max;
        // Проверяем ОБА диапазона — ошибка сбрасывается только если всё корректно
        self.error = self.input_Min >= self.input_Max || self.output_Min >= self.output_Max;
    }

    /// Задает новый диапазон выходного сигнала
    pub fn set_output_range(&mut self, output_Min : f32, output_Max : f32) {
        self.output_Min = output_Min;
        self.output_Max = output_Max;
        // Проверяем ОБА диапазона — ошибка сбрасывается только если всё корректно
        self.error = self.input_Min >= self.input_Max || self.output_Min >= self.output_Max;
    }


}
    