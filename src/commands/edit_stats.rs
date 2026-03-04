/// `/edit-stats` command group — admin only.
///
/// Provides four subcommands for managing the guild's stat XP configuration:
/// - `add`    — add a new stat with a given XP-per-unit value
/// - `edit`   — change the XP value for an existing stat
/// - `remove` — remove a stat (existing snapshots are kept)
/// - `list`   — display all configured stats and their XP values
///
/// All subcommands are ephemeral (visible only to the invoker) and require the
/// invoker's Discord user ID to be in `AppConfig.admin_user_ids`.
use poise::serenity_prelude::CreateEmbed;

use crate::config::GuildConfig;
use crate::database::queries;
use crate::shared::types::{Context, Error};

/// Discord stat names that live in their own table. They are valid targets for
/// `/edit-stats add` but are not looked up in the Hypixel stat key cache.
const DISCORD_STAT_NAMES: &[&str] = &["messages_sent", "reactions_added", "commands_used"];

// ---------------------------------------------------------------------------
// Autocomplete helpers
// ---------------------------------------------------------------------------

/// Autocomplete for any known stat key — includes all cached Bedwars keys plus
/// the three Discord stat names. Used by `/edit-stats add`.
///
/// Returns an empty list when `partial` is fewer than 2 characters, filters to
/// entries that *contain* the typed substring, sorts alphabetically, and caps
/// at 25 results (Discord's autocomplete limit).
async fn autocomplete_any_stat<'a>(ctx: Context<'_>, partial: &'a str) -> Vec<String> {
    if partial.len() < 2 {
        return Vec::new();
    }

    // Start with all Bedwars keys seen so far.
    let mut candidates: Vec<String> = {
        let known = ctx.data().hypixel.known_bedwars_stat_keys.read().await;
        known.clone()
    };

    // Append the three Discord stat names.
    for s in DISCORD_STAT_NAMES {
        candidates.push(s.to_string());
    }

    let partial_lower = partial.to_lowercase();
    let mut results: Vec<String> = candidates
        .into_iter()
        .filter(|k| k.to_lowercase().contains(&partial_lower))
        .collect();

    results.sort();
    results.dedup();
    results.truncate(25);
    results
}

/// Autocomplete for stats currently in the guild's `xp_config`. Used by
/// `/edit-stats edit` and `/edit-stats remove`.
///
/// Same length/filter/sort/cap rules as `autocomplete_any_stat`.
async fn autocomplete_configured_stat<'a>(ctx: Context<'_>, partial: &'a str) -> Vec<String> {
    if partial.len() < 2 {
        return Vec::new();
    }

    let guild_id = match ctx.guild_id() {
        Some(id) => id.get() as i64,
        None => return Vec::new(),
    };

    let config = match queries::get_guild(&ctx.data().db, guild_id).await {
        Ok(Some(row)) => {
            serde_json::from_str::<GuildConfig>(&row.config_json).unwrap_or_default()
        }
        _ => GuildConfig::default(),
    };

    let partial_lower = partial.to_lowercase();
    let mut results: Vec<String> = config
        .xp_config
        .keys()
        .filter(|k| k.to_lowercase().contains(&partial_lower))
        .cloned()
        .collect();

    results.sort();
    results.truncate(25);
    results
}

// ---------------------------------------------------------------------------
// Helper — inline admin check
// ---------------------------------------------------------------------------

/// Returns `true` when the command invoker is in the admin list.
fn is_admin(ctx: &Context<'_>) -> bool {
    ctx.data()
        .config
        .admin_user_ids
        .contains(&ctx.author().id.get())
}

// ---------------------------------------------------------------------------
// Parent command
// ---------------------------------------------------------------------------

/// Manage stat XP configuration for this server. Admin only.
#[poise::command(
    slash_command,
    guild_only,
    ephemeral,
    subcommands("add", "edit_stat", "remove", "list")
)]
pub async fn edit_stats(_ctx: Context<'_>) -> Result<(), Error> {
    // Parent command body is never called for subcommand groups.
    Ok(())
}

// ---------------------------------------------------------------------------
// Subcommands
// ---------------------------------------------------------------------------

/// Add a new stat to the XP configuration.
#[poise::command(slash_command, guild_only, ephemeral)]
pub async fn add(
    ctx: Context<'_>,
    #[description = "Stat name to track"]
    #[autocomplete = "autocomplete_any_stat"]
    stat_name: String,
    #[description = "XP awarded per unit increase"] xp_per_unit: f64,
) -> Result<(), Error> {
    if !is_admin(&ctx) {
        ctx.say("You do not have permission to use this command.")
            .await?;
        return Ok(());
    }

    let guild_id = ctx
        .guild_id()
        .ok_or("This command can only be used in a server")?
        .get() as i64;
    let data = ctx.data();

    queries::upsert_guild(&data.db, guild_id).await?;
    let guild_row = queries::get_guild(&data.db, guild_id).await?;
    let mut config: GuildConfig = guild_row
        .as_ref()
        .map(|g| serde_json::from_str(&g.config_json).unwrap_or_default())
        .unwrap_or_default();

    if config.xp_config.contains_key(&stat_name) {
        ctx.say(format!(
            "Stat `{}` is already configured ({} XP/unit). Use `/edit-stats edit` to change it.",
            stat_name,
            config.xp_config[&stat_name]
        ))
        .await?;
        return Ok(());
    }

    config.xp_config.insert(stat_name.clone(), xp_per_unit);
    let config_json = serde_json::to_string(&config)?;
    queries::update_guild_config(&data.db, guild_id, &config_json).await?;

    ctx.say(format!(
        "Added stat `{}` — **{} XP** per unit.",
        stat_name, xp_per_unit
    ))
    .await?;

    Ok(())
}

/// Edit the XP value for an existing stat.
#[poise::command(slash_command, guild_only, ephemeral, rename = "edit")]
pub async fn edit_stat(
    ctx: Context<'_>,
    #[description = "Stat to modify"]
    #[autocomplete = "autocomplete_configured_stat"]
    stat_name: String,
    #[description = "New XP per unit"] new_xp_value: f64,
) -> Result<(), Error> {
    if !is_admin(&ctx) {
        ctx.say("You do not have permission to use this command.")
            .await?;
        return Ok(());
    }

    let guild_id = ctx
        .guild_id()
        .ok_or("This command can only be used in a server")?
        .get() as i64;
    let data = ctx.data();

    queries::upsert_guild(&data.db, guild_id).await?;
    let guild_row = queries::get_guild(&data.db, guild_id).await?;
    let mut config: GuildConfig = guild_row
        .as_ref()
        .map(|g| serde_json::from_str(&g.config_json).unwrap_or_default())
        .unwrap_or_default();

    if !config.xp_config.contains_key(&stat_name) {
        ctx.say(format!(
            "Stat `{}` is not configured. Use `/edit-stats add` to add it.",
            stat_name
        ))
        .await?;
        return Ok(());
    }

    let old_xp = config.xp_config[&stat_name];
    config.xp_config.insert(stat_name.clone(), new_xp_value);
    let config_json = serde_json::to_string(&config)?;
    queries::update_guild_config(&data.db, guild_id, &config_json).await?;

    ctx.say(format!(
        "Updated `{}`: {} XP/unit → **{} XP/unit**.",
        stat_name, old_xp, new_xp_value
    ))
    .await?;

    Ok(())
}

/// Remove a stat from the XP configuration.
#[poise::command(slash_command, guild_only, ephemeral)]
pub async fn remove(
    ctx: Context<'_>,
    #[description = "Stat to remove"]
    #[autocomplete = "autocomplete_configured_stat"]
    stat_name: String,
) -> Result<(), Error> {
    if !is_admin(&ctx) {
        ctx.say("You do not have permission to use this command.")
            .await?;
        return Ok(());
    }

    let guild_id = ctx
        .guild_id()
        .ok_or("This command can only be used in a server")?
        .get() as i64;
    let data = ctx.data();

    queries::upsert_guild(&data.db, guild_id).await?;
    let guild_row = queries::get_guild(&data.db, guild_id).await?;
    let mut config: GuildConfig = guild_row
        .as_ref()
        .map(|g| serde_json::from_str(&g.config_json).unwrap_or_default())
        .unwrap_or_default();

    if config.xp_config.remove(&stat_name).is_none() {
        ctx.say(format!("Stat `{}` is not configured.", stat_name))
            .await?;
        return Ok(());
    }

    let config_json = serde_json::to_string(&config)?;
    queries::update_guild_config(&data.db, guild_id, &config_json).await?;

    ctx.say(format!(
        "Removed `{}` from XP configuration. Existing snapshots are preserved.",
        stat_name
    ))
    .await?;

    Ok(())
}

/// List all stats currently in the XP configuration.
#[poise::command(slash_command, guild_only, ephemeral)]
pub async fn list(ctx: Context<'_>) -> Result<(), Error> {
    if !is_admin(&ctx) {
        ctx.say("You do not have permission to use this command.")
            .await?;
        return Ok(());
    }

    let guild_id = ctx
        .guild_id()
        .ok_or("This command can only be used in a server")?
        .get() as i64;
    let data = ctx.data();

    let guild_row = queries::get_guild(&data.db, guild_id).await?;
    let config: GuildConfig = guild_row
        .as_ref()
        .map(|g| serde_json::from_str(&g.config_json).unwrap_or_default())
        .unwrap_or_default();

    if config.xp_config.is_empty() {
        ctx.say("No stats are currently configured for XP. Use `/edit-stats add` to add one.")
            .await?;
        return Ok(());
    }

    let mut lines: Vec<String> = config
        .xp_config
        .iter()
        .map(|(k, v)| format!("{k}: {v} XP/unit"))
        .collect();
    lines.sort();

    let embed = CreateEmbed::default()
        .title("Configured XP Stats")
        .description(format!("```\n{}\n```", lines.join("\n")))
        .color(0x00BFFF);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;

    Ok(())
}
