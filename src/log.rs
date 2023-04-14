use env_logger::Builder;
use env_logger::Env;

pub fn builder() -> Builder {
    let env = Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info");
    let mut b = Builder::from_env(env);
    b.format_level(true);
    b.format_timestamp_micros();
    b
}
