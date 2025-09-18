-- Add repo_path column to tasks table for multi-repo support
-- This allows tasks to specify a local repository path for container execution

ALTER TABLE tasks ADD COLUMN repo_path TEXT;