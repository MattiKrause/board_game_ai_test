mod impl1;
mod arena;
mod impl2;
mod impl3;
mod moves_buffer;
mod impl4;

pub use impl1::MonteCarloV2I1;
pub use impl2::MonteCarloV2I2;
pub use impl3::MonteCarloV2I3;
pub use impl4::{MonteCarloV2I4, MonteCarloConfigV2I4};