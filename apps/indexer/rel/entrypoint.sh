#!/bin/sh
set -e

echo "Creating database (if needed)..."
bin/opake_indexer eval "OpakeIndexer.Release.create_db()"

echo "Running migrations..."
bin/opake_indexer eval "OpakeIndexer.Release.migrate()"

echo "Starting indexer..."
exec bin/opake_indexer start
