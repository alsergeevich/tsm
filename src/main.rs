pub mod tsm;
use tsm::{TSM, Class, NumbersOfWire};

fn main() {
    // Создаем датчик: 50 Ом, класс B, 2 провода (длина 5м, сечение 0.75мм2), tau = 15 сек
    let mut sensor = TSM::new(
        50.0, 
        -50.0, 
        200.0, 
        Class::ClassB, 
        NumbersOfWire::Wire2, 
        5.0,  // длина
        0.25, // сечение
        20.0
    );

    println!("Симуляция: Нагрев среды с 0°C до 100°C");
    sensor.set_temperature_environment(100.0);

    let dt = 1.0; // Шаг 1 секунда
    println!("{:<10} | {:<15} | {:<15} | {:<15}", "Время (с)", "Реальная Т", "Показания", "Ошибка провода");
    println!("{:-<65}", "");

    let wire_error = sensor.get_wire_error_celsius();

    for t in 0..=60 {
        if t % 10 == 0 {
            println!(
                "{:<10} | {:<15.2} | {:<15.2} | {:<15.2}", 
                t, 
                sensor.get_sensor_temp(), 
                sensor.get_temperature(),
                wire_error
            );
        }
        sensor.tick(dt);
    }

    println!("\nАНАЛИЗ:");
    println!("1. При 2-х проводной схеме провода добавляют фиксированное смещение: {:.2}°C", wire_error);
    println!("2. Если заменить 50М на 100М, эта ошибка уменьшится в 2 раза.");
    println!("3. Если сменить схему на 3-х проводную, ошибка станет 0.00°C.");

}
