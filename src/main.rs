pub mod types;
pub mod wire;
pub mod meter;
pub mod rtd;

use types::{Class, TypeSensor};
use wire::{Wire, NumbersOfWire};
use meter::Meter;
use rtd::RTD;

fn main() {
    let sensor_type = TypeSensor::TypePt1000;

    // 1. Создаем чистый датчик
    let mut sensor = RTD::new(
        sensor_type, 
        -200.0, 
        200.0, 
        Class::ClassB, 
        40.0 // tau
    );

    // 2. Создаем линию связи (кабель 5м, сечение 0.75мм2, 2 провода)
    let cable = Wire::new(1.0, 0.75, NumbersOfWire::Wire2);

    // 3. Создаем измерительный прибор (настроен на тот же тип датчика)
    let plc = Meter::new(sensor_type);

    println!("Симуляция: Нагрев среды с 0°C до 100°C");
    sensor.set_temperature_environment(100.0);

    let dt = 1.0; // Шаг 1 секунда
    println!("{:<10} | {:<15} | {:<15} | {:<15} | {:<15}", "Время (с)", "Реальная Т", "Показания ПЛК", "Сырое R (Ом)", "R проводов");
    println!("{:-<80}", "");

    let r_wires = cable.get_resistance_wires();

    for t in 0..=60 {
        if t % 1 == 0 {
            // Читаем физическое сопротивление с клемм датчика
            let r_sensor = sensor.get_out_resistance_sensor();
            
            // Пропускаем сигнал через кабель
            let r_input_plc = cable.transmit(r_sensor);

            // ПЛК вычисляет температуру
            let t_measured = plc.measure(r_input_plc);

            println!(
                "{:<10} | {:<15.2} | {:<15.2} | {:<15.2} | {:<15.2}", 
                t, 
                sensor.get_real_sensor_temperature(), 
                t_measured,
                r_input_plc,
                r_wires
            );
        }
        sensor.tick(dt);
    }

    println!("\nАНАЛИЗ:");
    println!("1. Сопротивление проводов: {:.2} Ом", r_wires);
    println!("2. Мы построили полноценный цифровой двойник (Digital Twin) измерительного канала!");
}
