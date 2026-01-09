#!/bin/bash

# 旧版 xDS 集成测试脚本 - 已迁移到 tests/integration.sh
# 这个脚本保留用于向后兼容，建议使用新的测试套件

echo "⚠️  注意: 这是旧版的集成测试脚本"
echo "新的测试套件已迁移到 tests/ 目录"
echo ""
echo "推荐使用:"
echo "  make test              - 运行所有测试"
echo "  make test-integration  - 运行集成测试"
echo "  make test-unit         - 运行单元测试"
echo "  make test-performance  - 运行性能测试"
echo ""

read -p "是否继续运行旧版测试? (y/N): " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "已取消，请使用新的测试套件"
    exit 0
fi

echo "运行旧版集成测试..."
echo "===================="

# 执行新的集成测试
exec ./tests/integration.sh