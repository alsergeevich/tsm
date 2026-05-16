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
        3.0,  // длина
        0.15, // сечение
        20.0
    );

    println!("Симуляция: Нагрев среды с 0°C до 100°C");
    sensor.set_temperature_environment(100.0);

    let dt = 1.0; // Шаг 1 секунда
    println!("{:<10} | {:<20} | {:<20}", "Время (с)", "Реальная Т (°C)", "Показания (°C)");
    println!("{:-<55}", "");

    for t in 0..=60 {
        if t % 5 == 0 {
            // get_sensor_temp() - физическая температура датчика
            // get_temperature() - то, что выдает прибор (с учетом шума и проводов)
            println!(
                "{:<10} | {:<20.2} | {:<20.2}", 
                t, 
                sensor.get_sensor_temp(), 
                sensor.get_temperature()
            );
        }
        sensor.tick(dt);
    }

    println!("\nОбрати внимание, как показания прибора слегка 'дрожат' из-за шума,");
    println!("и как физическая температура плавно догоняет 100°C.");
}
