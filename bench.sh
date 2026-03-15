#!/bin/bash
mkdir -p uploads
echo "Creating 10000 dummy files..."
for i in {1..10000}; do
    touch "uploads/dummy${i}.txt"
done
echo "dummy file" > uploads/testfile.txt

# Start server in background
cargo run --release > /dev/null 2>&1 &
SERVER_PID=$!
sleep 3

echo "Benchmarking O(N) lookup..."
time curl -s http://localhost:3000/testfile > /dev/null

kill $SERVER_PID
rm -rf uploads
