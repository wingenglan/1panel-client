-- Adds per-second disk IO rates to sampled metrics for the dashboard monitor card.
ALTER TABLE metric_samples ADD COLUMN io_read_bytes_per_second INTEGER NOT NULL DEFAULT 0;
ALTER TABLE metric_samples ADD COLUMN io_write_bytes_per_second INTEGER NOT NULL DEFAULT 0;
