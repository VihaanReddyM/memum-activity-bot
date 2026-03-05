-- Migration 002: add minecraft_username to users table.
--
-- Stores the player's Minecraft display name at registration time so commands
-- can show it without a round-trip to the Mojang API on every invocation.
-- The column is nullable so existing rows continue to work (NULL = "Unknown").

ALTER TABLE users ADD COLUMN minecraft_username TEXT;
