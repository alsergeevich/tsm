//! # Основной модуль имитационного стенда
//!
//! Демонстрирует работу насоса в кольцевой противопожарной сети
//! с проверкой совместимости диаметров и подбором насоса.
//!
//! ИЗМЕНЕНИЯ:
//! - Добавлен NetworkType::Closed для кольцевой сети
//! - Добавлена передача диаметров труб для проверки совместимости
//! - Добавлен сценарий подбора насоса
//! - Использованы новые методы API

pub mod pump;
pub mod fluid_type;
pub mod pipeline;
pub mod system_traits;
pub mod system_functions;

use pump::{Pump, PumpSelector, PumpCandidate};
use fluid_type::FluidType;
use pipeline::{PipeSegmentBuilder, PipeMaterial, FittingType};
use system_traits::{PipeNetwork, HydraulicSystem};
use system_functions::NetworkType;

fn main() {
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║   ИМИТАЦИОННЫЙ СТЕНД НАСОСНОЙ УСТАНОВКИ                          ║");
    println!("║   Кольцевая противопожарная сеть DN339                           ║");
    println!("╚══════════════════════════════════════════════════════════════════╝\n");

    let fluid = FluidType::Water;
    let nominal_flow = 650.0;   // м³/ч
    let nominal_head = 32.0;    // м
    let pin_mpa = 0.2;          // МПа (статическое давление подпитки)
    
    // Параметры насоса
    let pump_inlet_mm = 339.0;
    let pump_outlet_mm = 339.0;
    let motor_power_kw = 75.0;
    let rpm = 1450.0;

    // =========================================================================
    // ПОСТРОЕНИЕ СЕТИ
    // =========================================================================
    
    // Функция создания переходного элемента (конфузор-горловина-диффузор)
    let make_transition = |d_main: f64, d_narrow: f64, length_throat: f64| {
        vec![
            PipeSegmentBuilder::default()
                .length(0.5)
                .material(PipeMaterial::SteelNew)
                .diameter(d_narrow)
                .inlet_diameter(d_main)
                .outlet_diameter(d_narrow)
                .fitting(FittingType::SuddenContraction, None)
                .build(),
            PipeSegmentBuilder::default()
                .length(length_throat)
                .diameter(d_narrow)
                .build(),
            PipeSegmentBuilder::default()
                .length(0.5)
                .diameter(d_narrow)
                .inlet_diameter(d_narrow)
                .outlet_diameter(d_main)
                .fitting(FittingType::SuddenExpansion, None)
                .build(),
        ]
    };

    let mut network_segments = Vec::new();

    // 5 магистральных сегментов по 200 м DN339 с переходами
    for i in 0..5 {
        network_segments.push(
            PipeSegmentBuilder::default()
                .length(200.0)
                .diameter(339.0)
                .build()
        );
        // Переход (кроме последнего сегмента — замыкаем без перехода)
        if i < 4 {
            network_segments.extend(make_transition(339.0, 339.0, 2.0));
        }
    }

    // ИЗМЕНЕНИЕ: Используем NetworkType::Closed для кольцевой сети
    let pipe_loop_length_m: f64 = network_segments.iter().map(|s| s.length_m).sum();
    let network = PipeNetwork::new(network_segments, pin_mpa, NetworkType::Closed);
    
    println!("Параметры сети:");
    println!("  Тип сети:        {:?}", network.network_type());
    println!("  Общая длина:     {:.0} м", pipe_loop_length_m);
    println!("  Диаметр труб:    {:.0} мм", network.first_pipe_diameter_mm());
    println!();

    // =========================================================================
    // СЦЕНАРИЙ 1: Остановленный насос
    // =========================================================================
    println!("┌──────────────────────────────────────────────────────────────────┐");
    println!("│ СЦЕНАРИЙ 1: СИМУЛЯЦИЯ ОСТАНОВЛЕННОГО НАСОСА                    │");
    println!("└──────────────────────────────────────────────────────────────────┘");
    
    let mut pump = Pump::new(
        pump_inlet_mm, pump_outlet_mm, pin_mpa,
        nominal_flow, nominal_head, motor_power_kw, rpm,
    );
    pump.set_running(false);
    
    // ИЗМЕНЕНИЕ: Передаем диаметры труб и тип сети
    let op_stopped = pump.find_working_point(
        &network, pin_mpa, fluid, pipe_loop_length_m,
        NetworkType::Closed,
        network.first_pipe_diameter_mm(),
        network.last_pipe_diameter_mm(),
    );
    
    println!("  Насос запущен:      false");
    println!("  Расход:             {:.1} м³/ч", op_stopped.flow_rate_m3h);
    println!("  Напор:              {:.1} м", op_stopped.head_m);
    println!("  Ошибка подключения: {}", op_stopped.connection_error);
    println!("  Система безопасна:  {}", op_stopped.system_is_safe);
    println!();

    // =========================================================================
    // СЦЕНАРИЙ 2: Запущенный насос
    // =========================================================================
    println!("┌──────────────────────────────────────────────────────────────────┐");
    println!("│ СЦЕНАРИЙ 2: РАБОТА НАСОСА В КОЛЬЦЕВОЙ СЕТИ                       │");
    println!("└──────────────────────────────────────────────────────────────────┘");
    
    let mut pump2 = Pump::new(
        pump_inlet_mm, pump_outlet_mm, pin_mpa,
        nominal_flow, nominal_head, motor_power_kw, rpm,
    );
    pump2.set_running(true);
    
    let mut op_running = pump2.find_working_point(
        &network, pin_mpa, fluid, pipe_loop_length_m,
        NetworkType::Closed,
        network.first_pipe_diameter_mm(),
        network.last_pipe_diameter_mm(),
    );
    
    // Фаза 2: анализ скоростей в сети
    let mut max_network_velocity = 0.0;
    for segment in &network.segments {
        let state = pipeline::PipeCalc::calculate(
            segment, op_running.flow_rate_m3h, op_running.outlet_pressure_mpa, 0.0, fluid
        );
        if state.velocity_ms > max_network_velocity {
            max_network_velocity = state.velocity_ms;
        }
    }
    op_running.update_system_diagnostics(max_network_velocity);
    
    println!("  Расход в кольце:        {:.1} м³/ч", op_running.flow_rate_m3h);
    println!("  Напор насоса:           {:.1} м", op_running.head_m);
    println!("  Давление на выходе:     {:.4} МПа", op_running.outlet_pressure_mpa);
    println!("  Давление на входе:      {:.4} МПа", op_running.inlet_pressure_mpa);
    println!("  Потребляемая мощность:  {:.2} кВт", op_running.shaft_power_kw);
    println!("  КПД:                    {:.1}%", op_running.efficiency * 100.0);
    println!("  Макс. скорость в сети:  {:.2} м/с", max_network_velocity);
    println!();
    println!("  Флаги безопасности:");
    println!("    Общая безопасность:   {}", op_running.system_is_safe);
    println!("    Перегрузка мотора:    {}", op_running.motor_overload_alarm);
    println!("    Превышение скорости:  {}", op_running.excessive_velocity_alarm);
    println!("    Кавитация:           {}", op_running.cavitation_risk);
    println!("    Ошибка подключения:   {}", op_running.connection_error);
    println!();
    println!("{}", op_running.diagnostics.generate_report(pipe_loop_length_m));
    
    // =========================================================================
    // СЦЕНАРИЙ 3: Тест ошибки подключения (диаметры несовместимы)
    // =========================================================================
    println!("\n┌──────────────────────────────────────────────────────────────────┐");
    println!("│ СЦЕНАРИЙ 3: ТЕСТ НЕСОВМЕСТИМОСТИ ДИАМЕТРОВ                      │");
    println!("└──────────────────────────────────────────────────────────────────┘");
    
    let mut pump3 = Pump::new(
        50.0, 50.0, pin_mpa,  // Насос с патрубками 50 мм
        nominal_flow, nominal_head, motor_power_kw, rpm,
    );
    pump3.set_running(true);
    
    let op_mismatch = pump3.find_working_point(
        &network, pin_mpa, fluid, pipe_loop_length_m,
        NetworkType::Closed,
        network.first_pipe_diameter_mm(),  // Труба 339 мм
        network.last_pipe_diameter_mm(),   // Труба 339 мм
    );
    
    println!("  Ошибка подключения: {}", op_mismatch.connection_error);
    println!("  Система безопасна:  {}", op_mismatch.system_is_safe);
    if op_mismatch.connection_error {
        println!("  ❌ Расчет заблокирован — диаметры несовместимы!");
        for w in &op_mismatch.diagnostics.warnings {
            println!("     {}", w.message());
        }
    }
    
    // =========================================================================
    // СЦЕНАРИЙ 4: Подбор насоса под сеть
    // =========================================================================
    println!("\n┌──────────────────────────────────────────────────────────────────┐");
    println!("│ СЦЕНАРИЙ 4: ПОДБОР НАСОСА ПОД ТРУБОПРОВОДНУЮ СЕТЬ             │");
    println!("└──────────────────────────────────────────────────────────────────┘");
    
    // Каталог доступных насосов (кандидаты)
    let candidates = vec![
        PumpCandidate {
            flow_m3h: 400.0, head_m: 20.0, power_kw: 45.0,
            din_mm: 250.0, dout_mm: 250.0, rpm: 1450.0,
            is_suitable: false, reason: String::new(),
        },
        PumpCandidate {
            flow_m3h: 500.0, head_m: 28.0, power_kw: 55.0,
            din_mm: 300.0, dout_mm: 300.0, rpm: 1450.0,
            is_suitable: false, reason: String::new(),
        },
        PumpCandidate {
            flow_m3h: 650.0, head_m: 32.0, power_kw: 75.0,
            din_mm: 339.0, dout_mm: 339.0, rpm: 1450.0,
            is_suitable: false, reason: String::new(),
        },
        PumpCandidate {
            flow_m3h: 800.0, head_m: 35.0, power_kw: 110.0,
            din_mm: 400.0, dout_mm: 350.0, rpm: 1450.0,
            is_suitable: false, reason: String::new(),
        },
    ];
    
    let selection = PumpSelector::select_pump(
        &network,
        &candidates,
        fluid,
        pin_mpa,
        NetworkType::Closed,
        network.first_pipe_diameter_mm(),
        network.last_pipe_diameter_mm(),
        pipe_loop_length_m,
    );
    
    println!("  Результат подбора: {}", selection.message);
    println!();
    
    for (i, candidate) in selection.candidates.iter().enumerate() {
        let status = if candidate.is_suitable { "✅ ПОДХОДИТ" } else { "❌ НЕ ПОДХОДИТ" };
        println!("  {}. Q={:.0} м³/ч, H={:.0} м, P={:.0} кВт, D={:.0}/{:.0} мм — {} ({})",
            i + 1,
            candidate.flow_m3h, candidate.head_m, candidate.power_kw,
            candidate.din_mm, candidate.dout_mm,
            status, candidate.reason
        );
    }
    
    if let Some(ref op) = selection.operating_point {
        println!();
        println!("  Параметры выбранного насоса в рабочей точке:");
        println!("    Расход:              {:.1} м³/ч", op.flow_rate_m3h);
        println!("    Напор:               {:.1} м", op.head_m);
        println!("    КПД:                 {:.1}%", op.efficiency * 100.0);
        println!("    Мощность на валу:    {:.2} кВт", op.shaft_power_kw);
        println!("    Система безопасна:   {}", op.system_is_safe);
    }

    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║                    СИМУЛЯЦИЯ ЗАВЕРШЕНА                           ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
}