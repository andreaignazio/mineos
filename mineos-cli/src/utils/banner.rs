use colored::*;
use std::time::Duration;
use std::thread;

/// Display the MineOS ASCII art banner
pub fn show_banner() {
    clear_screen();

    let banner = r#"
    ███╗   ███╗██╗███╗   ██╗███████╗ ██████╗ ███████╗
    ████╗ ████║██║████╗  ██║██╔════╝██╔═══██╗██╔════╝
    ██╔████╔██║██║██╔██╗ ██║█████╗  ██║   ██║███████╗
    ██║╚██╔╝██║██║██║╚██╗██║██╔══╝  ██║   ██║╚════██║
    ██║ ╚═╝ ██║██║██║ ╚████║███████╗╚██████╔╝███████║
    ╚═╝     ╚═╝╚═╝╚═╝  ╚═══╝╚══════╝ ╚═════╝ ╚══════╝
    "#;

    // Gradient colors from cyan to purple
    let lines: Vec<&str> = banner.trim().lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let colored_line = match i {
            0 => line.bright_cyan(),
            1 => line.cyan(),
            2 => line.bright_blue(),
            3 => line.blue(),
            4 => line.bright_magenta(),
            5 => line.magenta(),
            _ => line.white(),
        };
        println!("{}", colored_line);
    }

    println!();
    println!("{}", "    ⚡ Professional GPU Mining Engine ⚡".bright_yellow().bold());
    println!("{}", format!("            Version {}", env!("CARGO_PKG_VERSION")).bright_black());
    println!();
}

/// Show animated banner with typewriter effect
pub fn show_animated_banner() {
    clear_screen();

    let banner_lines = vec![
        "███╗   ███╗██╗███╗   ██╗███████╗ ██████╗ ███████╗",
        "████╗ ████║██║████╗  ██║██╔════╝██╔═══██╗██╔════╝",
        "██╔████╔██║██║██╔██╗ ██║█████╗  ██║   ██║███████╗",
        "██║╚██╔╝██║██║██║╚██╗██║██╔══╝  ██║   ██║╚════██║",
        "██║ ╚═╝ ██║██║██║ ╚████║███████╗╚██████╔╝███████║",
        "╚═╝     ╚═╝╚═╝╚═╝  ╚═══╝╚══════╝ ╚═════╝ ╚══════╝",
    ];

    // Animate each line appearing
    for (i, line) in banner_lines.iter().enumerate() {
        let colored_line = match i {
            0 => line.bright_cyan(),
            1 => line.cyan(),
            2 => line.bright_blue(),
            3 => line.blue(),
            4 => line.bright_magenta(),
            5 => line.magenta(),
            _ => line.white(),
        };

        // Typewriter effect
        for ch in colored_line.to_string().chars() {
            print!("{}", ch);
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
            thread::sleep(Duration::from_millis(2));
        }
        println!();
    }

    thread::sleep(Duration::from_millis(200));
    println!();

    // Fade in the tagline
    let tagline = "⚡ Professional GPU Mining Engine ⚡";
    println!("{}", tagline.bright_yellow().bold());

    thread::sleep(Duration::from_millis(100));
    println!("{}", format!("        Version {}", env!("CARGO_PKG_VERSION")).bright_black());
    println!();
}

/// Show a compact banner for quick commands
pub fn show_compact_banner() {
    println!("{} {} {}",
        "▶".bright_cyan(),
        "MineOS".bright_cyan().bold(),
        format!("v{}", env!("CARGO_PKG_VERSION")).bright_black()
    );
}

/// Show mining started animation
pub fn show_mining_animation() {
    let frames = vec!["⛏️ ", "⛏️ .", "⛏️ ..", "⛏️ ...", "⛏️ ....", "⛏️ ....."];

    for _ in 0..3 {
        for frame in &frames {
            print!("\r{} {}", frame, "Starting mining".bright_green());
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
            thread::sleep(Duration::from_millis(150));
        }
    }
    println!("\r{} {}", "⛏️", "Mining started successfully!".bright_green().bold());
}

/// Show GPU detection animation
pub fn show_gpu_detection_animation(gpu_count: usize) {
    let gpu_frames = vec!["🔍", "🔎", "🔍", "🔎"];

    for frame in gpu_frames.iter().cycle().take(8) {
        print!("\r{} Detecting GPUs...", frame);
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
        thread::sleep(Duration::from_millis(200));
    }

    println!("\r{} Found {} GPU{} ready for mining!",
        "✓".green().bold(),
        gpu_count.to_string().bright_cyan().bold(),
        if gpu_count != 1 { "s" } else { "" }
    );
}

/// Show hashrate with fancy formatting
pub fn show_hashrate(hashrate: f64) {
    let hashrate_str = format!("{:.2} MH/s", hashrate);
    let bar_length = (hashrate / 100.0 * 20.0) as usize;
    let bar = "█".repeat(bar_length);
    let empty = "░".repeat(20 - bar_length);

    println!("  {} {} {}{}",
        "Hashrate:".bold(),
        hashrate_str.bright_green().bold(),
        bar.bright_green(),
        empty.bright_black()
    );
}

/// Show temperature with color coding
pub fn show_temperature(temp: u32) {
    let temp_str = format!("{}°C", temp);
    let colored_temp = match temp {
        0..=60 => temp_str.green(),
        61..=75 => temp_str.yellow(),
        76..=85 => temp_str.bright_yellow(),
        _ => temp_str.red().bold(),
    };

    let icon = match temp {
        0..=60 => "❄️",
        61..=75 => "🌡️",
        76..=85 => "🔥",
        _ => "⚠️",
    };

    println!("  {} Temperature: {} {}", icon, colored_temp, get_temp_bar(temp));
}

fn get_temp_bar(temp: u32) -> String {
    let normalized = ((temp as f32 - 30.0) / 60.0 * 10.0) as usize;
    let bar_length = normalized.min(10);

    let bar = match temp {
        0..=60 => "▬".repeat(bar_length).green(),
        61..=75 => "▬".repeat(bar_length).yellow(),
        76..=85 => "▬".repeat(bar_length).bright_yellow(),
        _ => "▬".repeat(bar_length).red(),
    };

    let empty = "▬".repeat(10 - bar_length).bright_black();
    format!("[{}{}]", bar, empty)
}

/// Clear the screen
pub fn clear_screen() {
    print!("\x1B[2J\x1B[1;1H");
    std::io::Write::flush(&mut std::io::stdout()).unwrap();
}

/// Show a styled divider
pub fn show_divider() {
    println!("{}", "═".repeat(60).bright_black());
}

/// Show a fancy box around text
pub fn show_box(title: &str, content: Vec<&str>) {
    let max_len = content.iter().map(|s| s.len()).max().unwrap_or(0).max(title.len());
    let width = max_len + 4;

    // Top border
    println!("╔{}╗", "═".repeat(width).bright_cyan());

    // Title
    if !title.is_empty() {
        let padding = (width - title.len()) / 2;
        println!("║{}{}{} ║",
            " ".repeat(padding),
            title.bright_yellow().bold(),
            " ".repeat(width - padding - title.len())
        );
        println!("╠{}╣", "═".repeat(width).bright_cyan());
    }

    // Content
    for line in content {
        let padding = width - line.len();
        println!("║ {}{} ║", line, " ".repeat(padding - 1));
    }

    // Bottom border
    println!("╚{}╝", "═".repeat(width).bright_cyan());
}

/// Show a progress spinner
pub struct Spinner {
    frames: Vec<&'static str>,
    current: usize,
}

impl Spinner {
    pub fn new() -> Self {
        Self {
            frames: vec!["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
            current: 0,
        }
    }

    pub fn tick(&mut self, message: &str) {
        print!("\r{} {}", self.frames[self.current].bright_cyan(), message);
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
        self.current = (self.current + 1) % self.frames.len();
    }

    pub fn finish(&self, message: &str) {
        println!("\r{} {}", "✓".green().bold(), message);
    }
}