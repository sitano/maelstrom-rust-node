use env_logger::Builder;
use env_logger::Env;

pub fn builder() -> Builder {
    let mut b = Builder::from_env(Env::default());
    b.format_level(true);
    b.format_timestamp_micros();
    return b;
}
