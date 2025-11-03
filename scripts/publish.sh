#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}🚀 Publishing Agents Runtime SDK to Crates.io${NC}"
echo ""

# Check if logged in
if ! cargo login --help > /dev/null 2>&1; then
    echo -e "${RED}❌ Please login to crates.io first: cargo login${NC}"
    exit 1
fi

# Crates in dependency order
CRATES=(
    "agent-primitives"
    "agent-config"
    "agent-telemetry"
    "agent-tools-macros"
    "agent-tools"
    "agent-memory"
    "agent-policy"
    "agent-prompts"
    "agent-adapters"
    "agent-kernel"
    "mxp-agents"
)

# Dry run first
echo -e "${YELLOW}📋 Running dry-run for all crates...${NC}"
for crate in "${CRATES[@]}"; do
    echo -e "${YELLOW}  Checking $crate...${NC}"
    cd "$crate"
    if ! cargo publish --dry-run; then
        echo -e "${RED}❌ Dry-run failed for $crate${NC}"
        exit 1
    fi
    cd ..
done

echo -e "${GREEN}✅ All dry-runs passed!${NC}"
echo ""

# Ask for confirmation
read -p "Do you want to proceed with publishing? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${YELLOW}Aborted.${NC}"
    exit 0
fi

# Publish each crate
for crate in "${CRATES[@]}"; do
    echo -e "${GREEN}📦 Publishing $crate...${NC}"
    cd "$crate"
    
    if cargo publish; then
        echo -e "${GREEN}✅ Published $crate${NC}"
    else
        echo -e "${RED}❌ Failed to publish $crate${NC}"
        exit 1
    fi
    
    cd ..
    
    # Wait for crates.io to index (except for last crate)
    if [ "$crate" != "mxp-agents" ]; then
        echo -e "${YELLOW}⏳ Waiting 30 seconds for crates.io indexing...${NC}"
        sleep 30
    fi
done

echo ""
echo -e "${GREEN}🎉 All crates published successfully!${NC}"
echo ""
echo "Verify at:"
for crate in "${CRATES[@]}"; do
    echo "  https://crates.io/crates/$crate"
done

