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
    target_temp: f64,   // Текущая целевая температура (после учёта рампа)
    ramp_end_temp: f64, // Конечная температура линейного перехода
    ramp_rate: f64,     // Скорость изменения уставки [°C/с]
    is_ramping: bool,   // Флаг активного линейного перехода

    // --- Тепловая инерция (ОДУ 1-го порядка) ---
    temp_inertia: f64, // Температура после фильтра инерции
    lag_coeff: f64,    // Предвычисленный коэффициент дискретизации

    // --- Транспортная задержка (кольцевой буфер) ---
    delay_buffer: Vec<f64>, // История значений температуры
    write_idx: usize,       // Текущий индекс записи
    buffer_size: usize,     // Длина буфера (0 = задержка отключена)
    output_temp: f64,       // Кэшированный выходной сигнал (O(1) доступ)
}

/// Число шагов `dt` для кольцевого буфера задержки (ceil, без занижения τ).
fn delay_buffer_steps(transport_delay: f64, dt: f64) -> usize {
    if transport_delay <= 0.0 {
        return 0;
    }
    let steps = (transport_delay / dt).ceil() as usize;
    steps.max(1)
}

impl ThermalProcessSimulator {
    /// Создаёт симулятор с начальной температурой 20 °C.
    pub fn new(cfg: ProcessConfig) -> Result<Self, &'static str> {
        Self::new_with_initial(cfg, 20.0)
    }

    /// Создаёт симулятор с заданной начальной температурой [°C].
    pub fn new_with_initial(cfg: ProcessConfig, initial_temp: f64) -> Result<Self, &'static str> {
        if cfg.dt <= 0.0 || cfg.tau_process <= 0.0 {
            return Err("dt и tau_process должны быть строго > 0");
        }
        if cfg.transport_delay < 0.0 {
            return Err("transport_delay не может быть отрицательным");
        }

        let buf_len = delay_buffer_steps(cfg.transport_delay, cfg.dt);

        // Коэффициент точной дискретизации экспоненциального перехода
        // Выведен из точного решения ОДУ: T[n] = T[n-1] + (T_set - T[n-1]) * (1 - e^(-dt/τ))
        let lag_k = 1.0 - E.powf(-cfg.dt / cfg.tau_process);

        Ok(Self {
            config: cfg,
            target_temp: initial_temp,
            ramp_end_temp: initial_temp,
            ramp_rate: 0.0,
            is_ramping: false,
            temp_inertia: initial_temp,
            lag_coeff: lag_k,
            delay_buffer: vec![initial_temp; buf_len],
            write_idx: 0,
            buffer_size: buf_len,
            output_temp: initial_temp,
        })
    }

    /// Задаёт новую целевую температуру.
    ///
    /// # Аргументы
    /// * `new_temp` — целевая температура [°C]
    /// * `ramp_speed` — скорость изменения уставки [°C/с] (используется модуль; знак игнорируется).
    ///   `0.0` — мгновенная ступенька; `> 0` — линейный переход к `new_temp`
    ///   (направление нагрева/охлаждения определяется автоматически по текущей уставке).
    pub fn set_target(&mut self, new_temp: f64, ramp_speed: f64) {
        self.ramp_end_temp = new_temp;
        let speed = ramp_speed.abs();

        if speed > 1e-9 {
            let direction = if new_temp > self.target_temp {
                1.0
            } else if new_temp < self.target_temp {
                -1.0
            } else {
                self.target_temp = new_temp;
                self.is_ramping = false;
                self.ramp_rate = 0.0;
                return;
            };
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
        if self.is_ramping {
            self.target_temp += self.ramp_rate * self.config.dt;

            let reached = if self.ramp_rate > 0.0 {
                self.target_temp >= self.ramp_end_temp
            } else {
                self.target_temp <= self.ramp_end_temp
            };

            if reached {
                self.target_temp = self.ramp_end_temp;
                self.is_ramping = false;
            }
        }

        // =========================================================================
        // ЭТАП 2: ТЕПЛОВАЯ ИНЕРЦИЯ КОНТУРА (ФИЛЬТР 1-ГО ПОРЯДКА)
        // =========================================================================
        self.temp_inertia += (self.target_temp - self.temp_inertia) * self.lag_coeff;

        // =========================================================================
        // ЭТАП 3: ТРАНСПОРТНАЯ ЗАДЕРЖКА (ЧИСТОЕ ВРЕМЕННОЕ СМЕЩЕНИЕ)
        // =========================================================================
        if self.buffer_size == 0 {
            self.output_temp = self.temp_inertia;
        } else {
            self.output_temp = self.delay_buffer[self.write_idx];
            self.delay_buffer[self.write_idx] = self.temp_inertia;
            self.write_idx = (self.write_idx + 1) % self.buffer_size;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(dt: f64, tau: f64, delay: f64) -> ProcessConfig {
        ProcessConfig {
            dt,
            tau_process: tau,
            transport_delay: delay,
        }
    }

    #[test]
    fn zero_transport_delay_does_not_panic() {
        let mut sim = ThermalProcessSimulator::new(cfg(0.1, 10.0, 0.0)).unwrap();
        sim.set_target(50.0, 0.0);
        for _ in 0..100 {
            sim.step();
        }
        assert!(sim.get_physical_temperature().is_finite());
        assert!(sim.get_source_temperature().is_finite());
    }

    #[test]
    fn delay_buffer_steps_uses_ceil() {
        assert_eq!(delay_buffer_steps(0.0, 0.1), 0);
        assert_eq!(delay_buffer_steps(0.04, 0.1), 1);
        assert_eq!(delay_buffer_steps(0.25, 0.1), 3);
        assert_eq!(delay_buffer_steps(1.0, 0.1), 10);
    }

    #[test]
    fn exponential_step_response_no_delay() {
        let dt = 0.1;
        let tau = 10.0;
        let mut sim = ThermalProcessSimulator::new_with_initial(cfg(dt, tau, 0.0), 0.0).unwrap();
        sim.set_target(100.0, 0.0);

        let k = 1.0 - E.powf(-dt / tau);
        sim.step();
        let expected = 100.0 * k;
        assert!((sim.get_source_temperature() - expected).abs() < 1e-12);
        assert!((sim.get_physical_temperature() - expected).abs() < 1e-12);
    }

    #[test]
    fn ramp_reaches_target() {
        let dt = 0.1;
        let mut sim = ThermalProcessSimulator::new_with_initial(cfg(dt, 30.0, 0.0), 20.0).unwrap();
        sim.set_target(75.0, 1.5);

        let steps = ((75.0 - 20.0) / 1.5 / dt).ceil() as usize + 5;
        for _ in 0..steps {
            sim.step();
        }

        assert!((sim.target_temp - 75.0).abs() < 1e-9);
        assert!(!sim.is_ramping);
    }

    #[test]
    fn transport_delay_lags_source() {
        let dt = 0.1;
        let delay = 0.5;
        let steps = delay_buffer_steps(delay, dt);
        let mut sim =
            ThermalProcessSimulator::new_with_initial(cfg(dt, 30.0, delay), 20.0).unwrap();
        sim.set_target(80.0, 0.0);

        for _ in 0..steps {
            sim.step();
        }

        assert!(
            (sim.get_physical_temperature() - 20.0).abs() < 1e-9,
            "датчик ещё видит начальную температуру"
        );
        assert!(sim.get_source_temperature() > 20.0);
    }

    #[test]
    fn reset_restores_state() {
        let mut sim = ThermalProcessSimulator::new_with_initial(cfg(0.1, 10.0, 0.3), 20.0)
            .unwrap();
        sim.set_target(90.0, 0.0);
        for _ in 0..50 {
            sim.step();
        }
        sim.reset(15.0);
        assert!((sim.get_physical_temperature() - 15.0).abs() < 1e-12);
        assert!((sim.get_source_temperature() - 15.0).abs() < 1e-12);
        assert!(!sim.is_ramping);
    }

    #[test]
    fn invalid_config_returns_error() {
        assert!(ThermalProcessSimulator::new(cfg(0.0, 10.0, 0.0)).is_err());
        assert!(ThermalProcessSimulator::new(cfg(0.1, -1.0, 0.0)).is_err());
        assert!(ThermalProcessSimulator::new(cfg(0.1, 10.0, -0.1)).is_err());
    }
}
