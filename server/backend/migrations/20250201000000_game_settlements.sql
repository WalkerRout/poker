-- game settlements (tracks if a game is finalized/locked)
CREATE TABLE IF NOT EXISTS game_settlements (
    game_id UUID PRIMARY KEY REFERENCES games(id) ON DELETE CASCADE,
    settled BOOLEAN NOT NULL DEFAULT FALSE,
    settled_at TIMESTAMPTZ
);

-- backfill existing games as unsettled
INSERT INTO game_settlements (game_id, settled)
SELECT id, FALSE FROM games
ON CONFLICT (game_id) DO NOTHING;
