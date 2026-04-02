#!/bin/sh
set -e

echo "Creating database (if needed)..."
bin/opake_appview eval "OpakeAppview.Release.create_db()"

echo "Running migrations..."
bin/opake_appview eval "OpakeAppview.Release.migrate()"

echo "Starting appview..."
exec bin/opake_appview start
