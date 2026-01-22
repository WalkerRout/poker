-- players
CREATE TABLE IF NOT EXISTS players (
    id UUID PRIMARY KEY,
    first_name TEXT NOT NULL,
    last_name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- games
CREATE TABLE IF NOT EXISTS games (
    id UUID PRIMARY KEY,
    started_at TIMESTAMPTZ NOT NULL,
    ended_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- game entries (one per player per game)
CREATE TABLE IF NOT EXISTS game_entries (
    id UUID PRIMARY KEY,
    game_id UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    buy_in_cents INTEGER NOT NULL,
    winnings_cents INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(game_id, player_id)
);

-- indexes
CREATE INDEX IF NOT EXISTS idx_game_entries_game_id ON game_entries(game_id);
CREATE INDEX IF NOT EXISTS idx_game_entries_player_id ON game_entries(player_id);