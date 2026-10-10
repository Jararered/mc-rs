//! Multiplayer transports. The simulation knows nothing of them: a server
//! here drives a [`WorldHost`](crate::world::host::WorldHost) and turns
//! packets into the same messages a local player's input becomes.

pub mod beta;
