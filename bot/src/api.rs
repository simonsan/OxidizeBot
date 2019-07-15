mod base;
pub mod bttv;
pub mod ffz;
pub mod github;
pub mod nightbot;
pub mod open_weather_map;
pub mod setbac;
pub mod speedrun;
pub mod spotify;
pub mod tduva;
pub mod twitch;
pub mod youtube;

pub use self::base::RequestBuilder;
pub use self::bttv::BetterTTV;
pub use self::ffz::FrankerFaceZ;
pub use self::github::GitHub;
pub use self::nightbot::NightBot;
pub use self::open_weather_map::OpenWeatherMap;
pub use self::setbac::SetBac;
pub use self::speedrun::Speedrun;
pub use self::spotify::Spotify;
pub use self::tduva::Tduva;
pub use self::twitch::Twitch;
pub use self::youtube::YouTube;
