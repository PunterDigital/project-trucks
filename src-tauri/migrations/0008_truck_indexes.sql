-- Indexes for the truck-scoped queries (#5670): the Truck page's stats (`TruckScope` in
-- commands/stats.rs), the Jobs / Sessions truck filters and `rollups::recompute_truck` all filter
-- on `truck_make = ? AND truck_model = ?`, and deleting a session sets `jobs.session_id` to NULL
-- through the foreign key. Without these each of them reads the whole table.
CREATE INDEX idx_sessions_truck ON driving_sessions (truck_make, truck_model);
CREATE INDEX idx_jobs_truck ON jobs (truck_make, truck_model);
CREATE INDEX idx_jobs_session_id ON jobs (session_id);
CREATE INDEX idx_expenses_truck ON expenses (truck_make, truck_model);
