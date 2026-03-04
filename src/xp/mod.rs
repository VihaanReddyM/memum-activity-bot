/// XP and leveling system.
///
/// - `calculator` — converts stat deltas into XP and computes levels from
///   total XP using a configurable exponential curve.
pub mod calculator;

pub use self::calculator::XPConfig;
