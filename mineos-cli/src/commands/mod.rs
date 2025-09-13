pub mod start;
pub mod stop;
pub mod status;
pub mod benchmark;
pub mod setup;
pub mod config_cmd;
pub mod overclock;
pub mod switch;
pub mod profit;
pub mod update;

// Re-export command executors
pub use start::execute as start_mining;
pub use stop::execute as stop_mining;
pub use status::execute as show_status;