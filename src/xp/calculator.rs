use std::collections::HashMap;

use crate::shared::types::StatDelta;

/// XP configuration per stat (XP awarded per unit delta).
#[derive(Clone)]
pub struct XPConfig {
    pub rewards: HashMap<String, f64>,
}

impl XPConfig {
    pub fn new(rewards: HashMap<String, f64>) -> Self {
        Self { rewards }
    }
}

impl Default for XPConfig {
    fn default() -> Self {
        let mut rewards = HashMap::new();
        rewards.insert("wins".to_string(), 50.0);
        rewards.insert("kills".to_string(), 5.0);
        rewards.insert("beds_broken".to_string(), 25.0);
        rewards.insert("messages_sent".to_string(), 1.0);
        rewards.insert("reactions_added".to_string(), 1.0);
        rewards.insert("commands_used".to_string(), 2.0);
        XPConfig { rewards }
    }
}

/// Convert stat deltas into earned XP and return the total XP.
pub fn calculate_xp(deltas: &[StatDelta], config: &XPConfig) -> f64 {
    let mut earned: f64 = 0.0;
    for d in deltas {
        if d.difference <= 0.0 {
            continue;
        }
        if let Some(per_unit) = config.rewards.get(&d.stat_name) {
            let units = d.difference.round() as i64;
            earned += (units as f64) * per_unit;
        }
    }
    earned
}

/// Calculate the user's level from total XP using an exponential curve.
/// total_xp is the user's cumulative XP. base_xp is the XP required to reach level 2,
/// exponent is the exponential scaling factor.
pub fn calculate_level(total_xp: f64, base_xp: f64, exponent: f64) -> i32 {
    if total_xp < 0.0 {
        return 1;
    }
    let mut level: i32 = 1;
    loop {
        let next_level = level + 1;
        // XP required to reach the next level from the current level.
        // Based on: required_xp(level) = base_xp * ((level) ^ exponent)
        // We interpret the threshold for the next level as (next_level - 1).
        let needed = base_xp * (((next_level - 1) as f64).powf(exponent));
        if total_xp >= needed {
            level = next_level;
        } else {
            break;
        }
        if level > 1_000_000 {
            break;
        }
    }
    level
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::types::StatDelta;
    use std::collections::HashMap;

    fn default_config() -> XPConfig {
        XPConfig::default()
    }

    #[test]
    fn xp_no_deltas_returns_zero() {
        let config = default_config();
        assert_eq!(calculate_xp(&[], &config), 0.0);
    }

    #[test]
    fn xp_single_stat_delta() {
        let config = default_config();
        // 3 wins * 50 XP/win = 150 XP
        let deltas = vec![StatDelta::new(1, "wins".to_string(), 10.0, 13.0)];
        assert_eq!(calculate_xp(&deltas, &config), 150.0);
    }

    #[test]
    fn xp_multiple_stat_deltas() {
        let config = default_config();
        let deltas = vec![
            StatDelta::new(1, "wins".to_string(), 0.0, 2.0), // 2 * 50 = 100
            StatDelta::new(1, "kills".to_string(), 0.0, 10.0), // 10 * 5 = 50
            StatDelta::new(1, "beds_broken".to_string(), 0.0, 4.0), // 4 * 25 = 100
        ];
        assert_eq!(calculate_xp(&deltas, &config), 250.0);
    }

    #[test]
    fn xp_negative_delta_ignored() {
        let config = default_config();
        // Stat went down (possible API glitch) — should award 0 XP
        let deltas = vec![StatDelta::new(1, "wins".to_string(), 10.0, 8.0)];
        assert_eq!(calculate_xp(&deltas, &config), 0.0);
    }

    #[test]
    fn xp_unknown_stat_ignored() {
        let config = default_config();
        let deltas = vec![StatDelta::new(1, "unknown_stat".to_string(), 0.0, 100.0)];
        assert_eq!(calculate_xp(&deltas, &config), 0.0);
    }

    #[test]
    fn xp_discord_stats_included() {
        let config = default_config();
        let deltas = vec![
            StatDelta::new(1, "messages_sent".to_string(), 0.0, 5.0), // 5 * 1 = 5
            StatDelta::new(1, "reactions_added".to_string(), 0.0, 3.0), // 3 * 1 = 3
            StatDelta::new(1, "commands_used".to_string(), 0.0, 2.0), // 2 * 2 = 4
        ];
        assert_eq!(calculate_xp(&deltas, &config), 12.0);
    }

    #[test]
    fn xp_custom_config() {
        let mut rewards = HashMap::new();
        rewards.insert("wins".to_string(), 100.0);
        let config = XPConfig::new(rewards);

        let deltas = vec![StatDelta::new(1, "wins".to_string(), 0.0, 3.0)]; // 3 * 100 = 300
        assert_eq!(calculate_xp(&deltas, &config), 300.0);
    }

    // ---------------------------------------------------------------
    // calculate_level tests
    // ---------------------------------------------------------------

    #[test]
    fn level_zero_xp_is_level_one() {
        assert_eq!(calculate_level(0.0, 100.0, 1.5), 1);
    }

    #[test]
    fn level_negative_xp_is_level_one() {
        assert_eq!(calculate_level(-50.0, 100.0, 1.5), 1);
    }

    #[test]
    fn level_just_below_threshold_stays() {
        // Level 2 requires base_xp * (1^1.5) = 100 XP
        assert_eq!(calculate_level(99.9, 100.0, 1.5), 1);
    }

    #[test]
    fn level_exactly_at_threshold_advances() {
        // Level 2 requires 100 * (1^1.5) = 100 XP
        assert_eq!(calculate_level(100.0, 100.0, 1.5), 2);
    }

    #[test]
    fn level_well_above_threshold() {
        // Level 2 = 100, Level 3 = 100 * 2^1.5 = 282.84...
        // Level 4 = 100 * 3^1.5 = 519.61...
        // 500 XP should be level 3
        assert_eq!(calculate_level(500.0, 100.0, 1.5), 3);
    }

    #[test]
    fn level_large_xp() {
        // Sanity check that large XP values produce a reasonable level > 1
        let level = calculate_level(100_000.0, 100.0, 1.5);
        assert!(level > 1);
        assert!(level < 1_000_000);
    }

    #[test]
    fn level_linear_exponent() {
        // With exponent = 1.0, required_xp(level) = base * level
        // Level 2 = 100 * 1 = 100, Level 3 = 100 * 2 = 200, Level 4 = 100 * 3 = 300
        assert_eq!(calculate_level(0.0, 100.0, 1.0), 1);
        assert_eq!(calculate_level(100.0, 100.0, 1.0), 2);
        assert_eq!(calculate_level(200.0, 100.0, 1.0), 3);
        assert_eq!(calculate_level(299.0, 100.0, 1.0), 3);
        assert_eq!(calculate_level(300.0, 100.0, 1.0), 4);
    }
}
