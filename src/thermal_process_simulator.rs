//! ============================================================================
//! МОДУЛЬ: Симулятор теплогидравлического процесса (нагрев теплоносителя)
//! НАЗНАЧЕНИЕ: Генерация детерминированной физической температуры для тестов
//! ЯЗЫК: Rust (только стандартная библиотека)
//! ============================================================================

use std::f64::consts::E;

/// Конфигурация физического процесса.
/// Все параметры задаются в единицах СИ.
#[derive(Debug, Clone, Copy)]
pub struct ProcessConfig {
    /// Шаг моделирования [с]. Рекомендуется 0.1..1.0 с для тепловых систем.
    pub dt: f64,
    
    /// Постоянная времени нагрева/остывания контура [с].
    /// Определяется массой теплоносителя, мощностью источника и теплопотерями.
    /// Типичные значения: 30..300 с.
    pub tau_process: f64,
    
    /// Транспортная задержка потока от источника до точки измерения [с].
    /// Рассчитывается как: τ_transport = L_трубы [м] / v_потока [м/с]
    pub transport_delay: f64,
}

/// Симулятор физики процесса. 
/// Возвращает "идеальную" температуру среды без шума, дрейфа и квантования.
pub struct ThermalProcessSimulator {
    config: ProcessConfig,

    // --- Управление уставкой (сценарий) ---
    target_temp: f64,    // Текущая целевая температура (после учёта рампа)
    ramp_end_temp: f64,  // Конечная температура линейного перехода
    ramp_rate: f64,      // Скорость изменения уставки [°C/с]
    is_ramping: bool,    // Флаг активного линейного перехода

    // --- Тепловая инерция (ОДУ 1-го порядка) ---
    temp_inertia: f64,   // Температура после фильтра инерции
    lag_coeff: f64,      // Предвычисленный коэффициент дискретизации

    // --- Транспортная задержка (кольцевой буфер) ---
    delay_buffer: Vec<f64>, // История значений температуры
    write_idx: usize,       // Текущий индекс записи
    buffer_size: usize,     // Длина буфера
    output_temp: f64,       // Кэшированный выходной сигнал (O(1) доступ)
}

impl ThermalProcessSimulator {
    /// Создаёт симулятор. Проверяет физическую осмысленность параметров.
    pub fn new(cfg: ProcessConfig) -> Result<Self, &'static str> {
        if cfg.dt <= 0.0 || cfg.tau_process <= 0.0 {
            return Err("dt и tau_process должны быть строго > 0");
        }
        if cfg.transport_delay < 0.0 {
            return Err("transport_delay не может быть отрицательным");
        }

        // Размер буфера: сколько шагов dt помещается в физическую задержку + 1 для безопасности
        let buf_len = (cfg.transport_delay / cfg.dt).round() as usize;
        
        // Коэффициент точной дискретизации экспоненциального перехода
        // Выведен из точного решения ОДУ: T[n] = T[n-1] + (T_set - T[n-1]) * (1 - e^(-dt/τ))
        let lag_k = 1.0 - E.powf(-cfg.dt / cfg.tau_process);

        Ok(Self {
            config: cfg,
            target_temp: 20.0,
            ramp_end_temp: 20.0,
            ramp_rate: 0.0,
            is_ramping: false,
            temp_inertia: 20.0,
            lag_coeff: lag_k,
            delay_buffer: vec![20.0; buf_len],
            write_idx: 0,
            buffer_size: buf_len,
            output_temp: 20.0,
        })
    }

    /// Задаёт новую целевую температуру.
    /// 
    /// # Аргументы
    /// * `new_temp` — целевая температура [°C]
    /// * `ramp_speed` — абсолютная скорость изменения [°C/с] (всегда положительная). 
    ///   `0.0` = мгновенная ступенька, `>0` = плавный набор, `<0` = плавный сброс.
     pub fn set_target(&mut self, new_temp: f64, ramp_speed: f64) {
        self.ramp_end_temp = new_temp;
        let speed = ramp_speed.abs();
        
        if speed > 1e-9 {
            // Автоматически определяем знак направления изменения
            let direction = if new_temp >= self.target_temp { 1.0 } else { -1.0 };
            self.ramp_rate = speed * direction;
            self.is_ramping = true;
        } else {
            self.ramp_rate = 0.0;
            self.is_ramping = false;
            self.target_temp = new_temp;
        }
    }

    /// Выполняет один шаг моделирования физики.
    /// Вызывать строго с интервалом `config.dt` в основном цикле.
    pub fn step(&mut self) {
        // =========================================================================
        // ЭТАП 1: ФОРМИРОВАНИЕ УСТАВКИ (РАМП)
        // =========================================================================
        // Физика: Исполнительные механизмы (горелки, клапаны) меняют мощность 
        // не мгновенно, а с ограниченной скоростью.
        // Формула: T_set[n] = T_set[n-1] + v_ramp * dt
        if self.is_ramping {
            self.target_temp += self.ramp_rate * self.config.dt;

            // Проверка достижения конечной точки (с учётом направления изменения)
            let reached = if self.ramp_rate > 0.0 {
                self.target_temp >= self.ramp_end_temp
            } else {
                self.target_temp <= self.ramp_end_temp
            };

            // Фиксация на цели и отключение рампа
            if reached {
                self.target_temp = self.ramp_end_temp;
                self.is_ramping = false;
            }
        }

        // =========================================================================
        // ЭТАП 2: ТЕПЛОВАЯ ИНЕРЦИЯ КОНТУРА (ФИЛЬТР 1-ГО ПОРЯДКА)
        // =========================================================================
        // Физика: Нагрев воды и металла подчиняется уравнению теплового баланса.
        // Непрерывная модель: τ * dT/dt + T = T_set
        // Точное дискретное решение для шага dt:
        // T[n] = T[n-1] + (T_set - T[n-1]) * (1 - exp(-dt/τ))
        // Коэффициент (1 - exp(-dt/τ)) предвычислен в new() как lag_coeff.
        self.temp_inertia += (self.target_temp - self.temp_inertia) * self.lag_coeff;

        // =========================================================================
        // ЭТАП 3: ТРАНСПОРТНАЯ ЗАДЕРЖКА (ЧИСТОЕ ВРЕМЕННОЕ СМЕЩЕНИЕ)
        // =========================================================================
        // Физика: Частица воды движется по трубе со скоростью v. 
        // Датчик видит то, что вышло из котла τ_transport секунд назад.
        // Математически: y(t) = u(t - τ_d)
        // Реализация: Кольцевой FIFO-буфер. Читаем старое значение, пишем новое.
        
        // 1. Считываем значение, которое "дошло" до датчика в этот момент
        self.output_temp = self.delay_buffer[self.write_idx];
        
        // 2. Перезаписываем ячейку новым значением из инерции
        self.delay_buffer[self.write_idx] = self.temp_inertia;
        
        // 3. Сдвигаем индекс записи (с зацикливанием)
        self.write_idx = (self.write_idx + 1) % self.buffer_size;
    }

    /// Возвращает истинную физическую температуру теплоносителя в точке измерения.
    /// Это "эталонное" значение (Ground Truth) для расчёта ошибки вашей модели датчика.
    #[inline]
    pub fn get_physical_temperature(&self) -> f64 {
        self.output_temp
    }

    /// Возвращает физическую температуру на выходе из источника (до транспортной задержки).
    #[inline]
    pub fn get_source_temperature(&self) -> f64 {
        self.temp_inertia
    }

    /// Сброс симулятора к начальному состоянию.
    pub fn reset(&mut self, initial_temp: f64) {
        self.target_temp = initial_temp;
        self.ramp_end_temp = initial_temp;
        self.ramp_rate = 0.0;
        self.is_ramping = false;
        self.temp_inertia = initial_temp;
        self.delay_buffer.fill(initial_temp);
        self.write_idx = 0;
        self.output_temp = initial_temp;
    }
}
