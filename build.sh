#!/bin/bash
export PATH="/home/zhiqing/.cargo/bin:$PATH"
cd ~/crow-hub
cargo check 2>&1 > /tmp/cargo-result.log
echo "EXIT_CODE=$?" >> /tmp/cargo-result.log
