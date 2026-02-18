-- make ended_at nullable
ALTER TABLE games
ALTER COLUMN ended_at DROP NOT NULL;
