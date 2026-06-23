-- Runs once on first container init. Creates the restricted runtime role
-- the app connects as. Migrations still run as `postgres` (table owner).
-- The two-role split is what makes RLS actually enforce: the owner
-- role bypasses RLS, the runtime role does not.

CREATE ROLE bf_app WITH LOGIN PASSWORD 'bf_app';
GRANT CONNECT ON DATABASE box_fraise TO bf_app;
GRANT USAGE ON SCHEMA public TO bf_app;
