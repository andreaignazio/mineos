#!/bin/bash

echo "=== MineOS Fancy CLI Demo ==="
echo ""

echo "1. Showing help with compact banner:"
./target/release/mineos --version
echo ""
sleep 2

echo "2. Status command with styled output:"
./target/release/mineos status
echo ""
sleep 2

echo "3. The fancy features include:"
echo "   ✨ Animated ASCII art banner"
echo "   🎨 Gradient colored logo"
echo "   📦 Styled boxes for important info"
echo "   🔄 Animated spinners and progress"
echo "   🌡️ Color-coded temperature displays"
echo "   ⛏️ Mining animations"
echo "   📊 Progress bars with custom styles"
echo ""

echo "Try these commands to see the fancy UI:"
echo "  ./target/release/mineos setup    # Animated banner + setup wizard"
echo "  ./target/release/mineos start    # Mining animations"
echo "  ./target/release/mineos dashboard # Real-time TUI"