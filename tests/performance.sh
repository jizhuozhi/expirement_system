#!/bin/bash

# 性能测试套件 - 基于现有的 load_test.sh 优化
set -e

echo "=== 性能测试套件 ==="

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# 默认配置
BASE_URL="${BASE_URL:-http://localhost:8080}"
DURATION="${DURATION:-30}"
CONCURRENCY="${CONCURRENCY:-50}"
WARMUP_DURATION="${WARMUP_DURATION:-10}"

echo "性能测试配置:"
echo "  目标地址: $BASE_URL"
echo "  测试时长: ${DURATION}s"
echo "  并发数: $CONCURRENCY"
echo "  预热时长: ${WARMUP_DURATION}s"
echo ""

# 检查依赖
check_dependency() {
    if ! command -v $1 &> /dev/null; then
        echo -e "${RED}✗ $1 未安装${NC}"
        return 1
    else
        echo -e "${GREEN}✓ $1 可用${NC}"
        return 0
    fi
}

echo -e "${BLUE}1. 检查依赖...${NC}"
check_dependency "hey" || {
    echo "安装 hey: go install github.com/rakyll/hey@latest"
    exit 1
}
check_dependency "curl" || exit 1

# 检查服务可用性
echo -e "\n${BLUE}2. 检查服务状态...${NC}"
if curl -s "$BASE_URL/health" > /dev/null; then
    echo -e "${GREEN}✓ 服务正常运行${NC}"
else
    echo -e "${RED}✗ 服务未运行或不可访问${NC}"
    echo "请先启动服务: make run"
    exit 1
fi

# 创建测试请求
create_test_requests() {
    # 实验查询请求
    cat > /tmp/experiment_request.json <<EOF
{
  "services": ["payment", "recommendation"],
  "context": {
    "user_id": "perf_test_user_123",
    "device_type": "mobile",
    "region": "us-west"
  }
}
EOF

    # 简单健康检查请求
    cat > /tmp/health_request.json <<EOF
{}
EOF
}

# 预热服务
warmup_service() {
    echo -e "\n${BLUE}3. 预热服务 (${WARMUP_DURATION}s)...${NC}"
    hey -z ${WARMUP_DURATION}s -c 10 \
        -m GET \
        "$BASE_URL/health" > /dev/null 2>&1
    echo -e "${GREEN}✓ 预热完成${NC}"
}

# 运行性能测试
run_performance_test() {
    local test_name="$1"
    local method="$2"
    local endpoint="$3"
    local data_file="$4"
    
    echo -e "\n${BLUE}测试: $test_name${NC}"
    echo "======================================"
    
    local cmd="hey -z ${DURATION}s -c ${CONCURRENCY} -m $method"
    
    if [ "$method" = "POST" ] && [ -n "$data_file" ]; then
        cmd="$cmd -H 'Content-Type: application/json' -D $data_file"
    fi
    
    cmd="$cmd $BASE_URL$endpoint"
    
    echo "执行命令: $cmd"
    echo ""
    
    eval $cmd
    
    echo ""
}

# 创建测试请求文件
create_test_requests

# 预热服务
warmup_service

# 运行各种性能测试
echo -e "\n${BLUE}4. 开始性能测试...${NC}"

# 1. 健康检查性能测试
run_performance_test "健康检查" "GET" "/health" ""

# 2. 实验查询性能测试
run_performance_test "实验查询" "POST" "/experiment" "/tmp/experiment_request.json"

# 3. 指标查询性能测试
run_performance_test "指标查询" "GET" "/metrics" ""

# 4. 高并发测试
echo -e "\n${BLUE}5. 高并发测试 (并发数: $((CONCURRENCY * 2)))...${NC}"
ORIGINAL_CONCURRENCY=$CONCURRENCY
CONCURRENCY=$((CONCURRENCY * 2))

run_performance_test "高并发实验查询" "POST" "/experiment" "/tmp/experiment_request.json"

CONCURRENCY=$ORIGINAL_CONCURRENCY

# 6. 压力测试 - 逐步增加负载
echo -e "\n${BLUE}6. 压力测试 - 逐步增加负载...${NC}"
for load in 10 25 50 100 200; do
    if [ $load -le $((CONCURRENCY * 4)) ]; then
        echo -e "\n${YELLOW}负载级别: $load 并发${NC}"
        hey -z 15s -c $load \
            -m POST \
            -H "Content-Type: application/json" \
            -D /tmp/experiment_request.json \
            "$BASE_URL/experiment" | grep -E "(Requests/sec|Latency|Status)"
    fi
done

# 清理临时文件
cleanup() {
    rm -f /tmp/experiment_request.json
    rm -f /tmp/health_request.json
}

cleanup

echo -e "\n${GREEN}=== 性能测试完成 ===${NC}"
echo -e "${YELLOW}建议:${NC}"
echo "  - 查看详细指标: curl $BASE_URL/metrics"
echo "  - 监控系统资源使用情况"
echo "  - 根据结果调整服务配置"
echo ""
echo -e "${BLUE}性能基准参考:${NC}"
echo "  - 健康检查: > 1000 RPS"
echo "  - 实验查询: > 500 RPS"
echo "  - P99 延迟: < 100ms"