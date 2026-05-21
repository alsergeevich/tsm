#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
pub mod types;
pub mod wire;
pub mod meter;
pub mod rtd;
pub mod analog_scaling_converter;
pub mod thermal_process_simulator;

use types::{Class, TypeSensor};
use rtd::RTD;
use meter::Meter;
use analog_scaling_converter::AnalogScaling;
use thermal_process_simulator::{ThermalProcessSimulator, ProcessConfig};

fn main() {
    // -------------------------------------------------------------------------
    // 1. ИСХОДНЫЕ ФИЗИЧЕСКИЕ ДАННЫЕ И РАСЧЕТ ТРАНСПОРТНОЙ ЗАДЕРЖКИ
    // -------------------------------------------------------------------------
    let l_pipe = 1.5;            // Расстояние от котла до датчика [м]
    let flow_rate_m3_h = 45.0;   // Расход воды через котел [м3/ч]
    
    // Выберем стандартный диаметр трубы для такого расхода: DN65 (внутренний диаметр ~65 мм)
    let d_inner = 0.065;         // Внутренний диаметр трубы [м]
    
    // Площадь поперечного сечения трубы S [м2]
    let s_area = std::f64::consts::PI * d_inner * d_inner / 4.0;
    
    // Расход в кубических метрах в секунду [м3/с]
    let flow_rate_m3_s = flow_rate_m3_h / 3600.0;
    
    // Скорость потока теплоносителя v = Q / S [м/с]
    let v_flow = flow_rate_m3_s / s_area;
    
    // Транспортная задержка tau_transport = L / v [с]
    let tau_transport = l_pipe / v_flow;
    let initial_temperature_coolant = 20.0; // °C
    let target_temperature_coolant = 75.0; // °C
    let ramp_speed_coolant = 1.5; // °C/s
    
    

    println!("================= НАСТРОЙКА ФИЗИЧЕСКОЙ МОДЕЛИ КОТЛА =================");
    println!("Расход теплоносителя:      {:.1} м3/ч ({:.5} м3/с)", flow_rate_m3_h, flow_rate_m3_s);
    println!("Внутренний диаметр трубы:  {:.3} м ({} мм)", d_inner, (d_inner * 1000.0) as i32);
    println!("Скорость движения потока:  {:.3} м/с", v_flow);
    println!("Дистанция до датчика (L):  {:.2} м", l_pipe);
    println!("Транспортная задержка:     {:.3} с", tau_transport);
    println!("=====================================================================");

    // -------------------------------------------------------------------------
    // 2. КОНФИГУРАЦИЯ СИМУЛЯТОРА И СЕНСОРНОЙ ЦЕПОЧКИ
    // -------------------------------------------------------------------------
    let dt = 0.1; // Шаг моделирования [с]
    
    // Симулятор физики котла:
    // Постоянная времени котла (tau_process) = 30.0 с (тепловая инерция нагревателя)
    let process_config = ProcessConfig {
        dt,
        tau_process: 30.0,
        transport_delay: tau_transport,
    };
    
    let mut boiler_sim = ThermalProcessSimulator::new(process_config)
        .expect("Ошибка конфигурации симулятора процесса");
        
    // Начальная температура котла и теплоносителя = 20.0 °C
    boiler_sim.reset(initial_temperature_coolant);
    
    // Датчик Pt100, класс А, постоянная времени гильзы датчика (tau) = 5.0 с.
    // Диапазон измерения умного датчика: от 0.0 до 100.0 °C.
    let t_min = 0.0;
    let t_max = 100.0;
    
    let mut sensor = RTD::new(
        TypeSensor::TypePt100,
        t_min,
        t_max,
        Class::ClassA,
        1.0, // tau_sensor = 1.0 с
    );
    sensor.set_temperature_environment(initial_temperature_coolant as f32);
    
    // Прогреваем датчик до начальной температуры воды (20.0 °C)
    for _ in 0..100 {
        sensor.tick(dt as f32);
    }
    
    // Измеритель ТСПУ (микропроцессорная часть: перевод сопротивления в градусы по ГОСТ)
    let smart_meter = Meter::new(TypeSensor::TypePt100);
    
    // Аналоговый преобразователь ТСПУ: измеренная температура -> ток 4..20 мА
    let mut tspu_current_output = AnalogScaling::new(
        t_min,
        t_max,
        4.0,
        20.0,
    );
    
    // Входной аналоговый модуль ПЛК: ток 4..20 мА -> температура 0..100 °C
    let mut plc_analog_input = AnalogScaling::new(
        4.0,
        20.0,
        t_min,
        t_max,
    );

    // -------------------------------------------------------------------------
    // 3. СИМУЛЯЦИОННЫЙ ЦИКЛ НАГРЕВА
    // -------------------------------------------------------------------------
    // Сценарий: 
    // - До t = 5.0 с система стабильна при 20.0 °C.
    // - При t = 5.0 с задаем нагрев котла до 75.0 °C.
    //   Скорость набора температуры (рамп горелки) = 1.5 °C/с.
    // - Общее время симуляции: 120 секунд.
    
    let total_simulation_time = 120.0; // секунд
    let steps_count = (total_simulation_time / dt) as usize;
    
    // Начальная уставка
    boiler_sim.set_target(initial_temperature_coolant, 0.0); // пуск
    
    println!("\n=== СИМУЛЯЦИЯ ПРОЦЕССА НАГРЕВА (Шаг dt = {:.2} с) ===", dt);
    println!("{:<9} | {:<12} | {:<12} | {:<12} | {:<12} | {:<9} | {:<8} | {:<12}", 
             "Время (с)", "Котел (°C)", "У датчика(°)", "Датчик (°C)", "R_датч (Ом)", "Ток (мА)", "ПЛК (°C)", "Ошибка (°C)");
    println!("{:-<105}", "");

    for step in 0..steps_count {
        let current_time = (step as f64) * dt;
        
        // Включаем нагрев на 5-й секунде
        if (current_time - 5.0).abs() < 1e-9 {
            boiler_sim.set_target(target_temperature_coolant, ramp_speed_coolant);
            println!(">>> [t = 5.0 с] Задаем нагрев котла до 75.0 °C со скоростью 1.5 °C/с");
            println!("{:-<105}", "");
        }
        
        // Шаг физического симулятора (нагрев котла + продвижение по трубе)
        boiler_sim.step();
        
        let t_boiler_exit = boiler_sim.get_source_temperature();       // Температура на выходе котла
        let t_at_sensor = boiler_sim.get_physical_temperature();       // Температура воды, дошедшая до датчика
        
        // Передаем физическую температуру воды в датчик
        sensor.set_temperature_environment(t_at_sensor as f32);
        sensor.tick(dt as f32); // Инерция датчика
        
        // Получаем сопротивление с шумами и погрешностями
        let r_sensor = sensor.get_out_resistance_sensor();
        
        // ТСПУ вычисляет температуру по сопротивлению
        let t_measured = smart_meter.measure(r_sensor);
        
        // Преобразование в токовую петлю 4..20 мА
        tspu_current_output.set_value_input(t_measured);
        let current_ma = tspu_current_output.get_value_output();
        
        // ПЛК пересчитывает ток обратно в показания температуры
        plc_analog_input.set_value_input(current_ma);
        let t_plc = plc_analog_input.get_value_output();
        
        // Ошибка всей измерительной цепочки относительно реальной температуры теплоносителя в месте установки датчика
        let total_system_error = t_plc - t_at_sensor as f32;

        // Выводим данные в лог каждые 5 секунд (50 шагов)
        if step % 50 == 0 || step == steps_count - 1 {
            println!(
                "{:<9.1} | {:<12.2} | {:<12.2} | {:<12.2} | {:<12.2} | {:<9.3} | {:<8.2} | {:<+12.3}",
                current_time,
                t_boiler_exit,
                t_at_sensor,
                sensor.get_real_sensor_temperature(),
                r_sensor,
                current_ma,
                t_plc,
                total_system_error
            );
        }
    }
    println!("{:-<105}", "");
    println!("Симуляция успешно завершена.");
}
