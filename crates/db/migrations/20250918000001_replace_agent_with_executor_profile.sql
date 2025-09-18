-- Replace the simple agent field with full executor profile support
-- This moves the executor profile concept from project level to task level

-- Drop the simple agent column
ALTER TABLE tasks DROP COLUMN agent;

-- Add executor profile columns
ALTER TABLE tasks ADD COLUMN executor TEXT;
ALTER TABLE tasks ADD COLUMN executor_variant TEXT;