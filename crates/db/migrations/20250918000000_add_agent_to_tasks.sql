-- Add agent column to tasks table for per-task agent selection
-- This allows each task to specify its own agent instead of inheriting from project profile

ALTER TABLE tasks ADD COLUMN agent TEXT;