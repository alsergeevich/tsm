#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
pub mod types;
pub mod wire;
pub mod meter;
pub mod rtd;
pub mod analog_scaling_converter;
pub mod thermal_process_simulator;
pub mod pid;
 
use types::{Class, TypeSensor};
use rtd::RTD;
use meter::Meter;
use analog_scaling_converter::AnalogScaling;
use thermal_process_simulator::{ThermalProcessSimulator, ProcessConfig};
use pid::PidController;

fn tau_transport(l_pipe: f64, flow_rate_m3_h: f64, s_area: f64) -> f64 {
    l_pipe / (flow_rate_m3_h / 3600.0 / s_area)
}

fn main() {
    
    // -------------------------------------------------------------------------
    // 1. ИСХОДНЫЕ ФИЗИЧЕСКИЕ ДАННЫЕ И РАСЧЕТ ТРАНСПОРТНОЙ ЗАДЕРЖКИ
    // -------------------------------------------------------------------------
    let l_pipe = 1.5;            // Расстояние от котла до датчика [м]
    let flow_rate_m3_h = 45.0;   // Расход воды через котел [м3/ч]
    
    let d_inner = 0.065;         // Внутренний диаметр трубы [м]
    let s_area = std::f64::consts::PI * d_inner * d_inner / 4.0;
    let tau_transport = tau_transport(l_pipe, flow_rate_m3_h, s_area);
    let initial_temperature_coolant = 20.0;
    let target_temperature_coolant = 75.0;
    
    
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
    let dt = 0.1;
    
    let process_config = ProcessConfig {
        dt,
        tau_process: 30.0,
        transport_delay: tau_transport,
    };
    
    let mut boiler_sim = ThermalProcessSimulator::new(process_config)
        .expect("Ошибка конфигурации симулятора процесса");
    boiler_sim.reset(initial_temperature_coolant);
    
    let t_min = 0.0;
    let t_max = 100.0;
    
    let mut sensor = RTD::new(
        TypeSensor::TypePt100, t_min, t_max, Class::ClassA, 1.0,
    );
    sensor.set_temperature_environment(initial_temperature_coolant as f32);
    for _ in 0..100 { sensor.tick(dt as f32); }
    
    let smart_meter = Meter::new(TypeSensor::TypePt100);
    let mut tspu_current_output = AnalogScaling::new(t_min, t_max, 4.0, 20.0);
    let mut plc_analog_input = AnalogScaling::new(4.0, 20.0, t_min, t_max);

    let mut pid = PidController::new(dt);
    pid.set_parameters(
        2.0, 0.05, 1.0, 5.0,
        -0.3, 0.3, 0.0, 100.0,
    );
    pid.update_inputs(initial_temperature_coolant, initial_temperature_coolant, 0.0, false);
    pid.tick();

    // -------------------------------------------------------------------------
    // 3. СИМУЛЯЦИОННЫЙ ЦИКЛ НАГРЕВА
    // -------------------------------------------------------------------------
    let total_simulation_time = 120.0;
    let steps_count = (total_simulation_time / dt) as usize;
    
    println!("\n=== СИМУЛЯЦИЯ ПРОЦЕССА НАГРЕВА (Шаг dt = {:.2} с) ===", dt);
    println!("{:<9} | {:<12} | {:<12} | {:<12} | {:<12} | {:<9} | {:<8} | {:<12} | {:<10}", 
             "Время (с)", "Котел (°C)", "У датчика(°)", "Датчик (°C)", "R_датч (Ом)", "Ток (мА)", "ПЛК (°C)", "Ошибка (°C)", "PID OP(%)");
    println!("{:-<118}", "");

    for step in 0..steps_count {
        let current_time = (step as f64) * dt;
        
        boiler_sim.step();
        let t_boiler_exit = boiler_sim.get_source_temperature();
        let t_at_sensor = boiler_sim.get_physical_temperature();
        
        sensor.set_temperature_environment(t_at_sensor as f32);
        sensor.tick(dt as f32);
        let r_sensor = sensor.get_out_resistance_sensor();
        let t_measured = smart_meter.measure(r_sensor);
        
        tspu_current_output.set_value_input(t_measured);
        let current_ma = tspu_current_output.get_value_output();
        
        plc_analog_input.set_value_input(current_ma);
        let t_plc = plc_analog_input.get_value_output();
        let total_system_error = t_plc - t_at_sensor as f32;

        pid.update_inputs(target_temperature_coolant, t_plc as f64, 0.0, false);
        pid.tick();
        let pid_op = pid.get_output();
        let ramp_speed = pid_op * 3.0 / 100.0;
        boiler_sim.set_target(target_temperature_coolant, ramp_speed);

        if step % 50 == 0 || step == steps_count - 1 {
            println!(
                "{:<9.1} | {:<12.2} | {:<12.2} | {:<12.2} | {:<12.2} | {:<9.3} | {:<8.2} | {:<+12.3} | {:<10.1}",
                current_time, t_boiler_exit, t_at_sensor,
                sensor.get_real_sensor_temperature(), r_sensor,
                current_ma, t_plc, total_system_error, pid_op
            );
        }
    }
    println!("{:-<118}", "");
    println!("Симуляция успешно завершена.");
}

