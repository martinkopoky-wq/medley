//! Music service integrations. Each implements [`medley_core::Provider`].

pub mod apple;
pub mod http;
pub mod soundcloud;
pub mod spotify;

pub use apple::AppleMusic;
pub use soundcloud::SoundCloud;
pub use spotify::Spotify;
