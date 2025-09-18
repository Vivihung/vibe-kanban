-- Simplify executor profile storage by using JSON
-- This makes the implementation much cleaner and aligns better with the existing ExecutorProfileId structure

-- Drop the separate executor columns
ALTER TABLE tasks DROP COLUMN executor;
ALTER TABLE tasks DROP COLUMN executor_variant;

-- Add a single JSON column for executor profile
ALTER TABLE tasks ADD COLUMN executor_profile_id TEXT;