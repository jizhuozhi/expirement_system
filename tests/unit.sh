#!/bin/bash

# 单元测试套件
set -e

echo "=== 单元测试套件 ==="

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# 测试结果统计
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

run_test() {
    local test_name="$1"
    local test_command="$2"
    
    echo -n "运行 $test_name... "
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    if eval "$test_command" > /dev/null 2>&1; then
        echo -e "${GREEN}✓ 通过${NC}"
        PASSED_TESTS=$((PASSED_TESTS + 1))
    else
        echo -e "${RED}✗ 失败${NC}"
        FAILED_TESTS=$((FAILED_TESTS + 1))
        echo "  命令: $test_command"
    fi
}

# 1. 控制面单元测试
echo -e "\n1. 控制面单元测试"
echo "==================="
cd control_plane
run_test "Go 单元测试" "go test -v ./..."
run_test "Go 代码格式检查" "go fmt ./... && git diff --exit-code"
run_test "Go 静态分析" "go vet ./..."
cd ..

# 2. 数据面单元测试
echo -e "\n2. 数据面单元测试"
echo "=================="
cd data_plane
run_test "Rust 单元测试" "cargo test --features grpc"
run_test "Rust 代码格式检查" "cargo fmt -- --check"
run_test "Rust Clippy 检查" "cargo clippy --features grpc -- -D warnings"
run_test "Rust 文档测试" "cargo test --doc --features grpc"
cd ..

# 3. Proto 验证
echo -e "\n3. Protocol Buffers 验证"
echo "========================="
run_test "Proto 语法检查" "protoc --proto_path=proto --descriptor_set_out=/dev/null proto/*.proto"
run_test "Proto 代码生成" "cd control_plane && make proto"

# 输出测试结果
echo -e "\n=== 测试结果 ==="
echo "总测试数: $TOTAL_TESTS"
echo -e "通过: ${GREEN}$PASSED_TESTS${NC}"
echo -e "失败: ${RED}$FAILED_TESTS${NC}"

if [ $FAILED_TESTS -eq 0 ]; then
    echo -e "\n${GREEN}🎉 所有单元测试通过！${NC}"
    exit 0
else
    echo -e "\n${RED}❌ 有 $FAILED_TESTS 个测试失败${NC}"
    exit 1
fi