#!/bin/bash

# 简单负载测试脚本 - 已迁移到 tests/performance.sh
# 这个脚本保留用于快速测试，完整的性能测试请使用: make test-performance

echo "⚠️  注意: 这是简化版的负载测试"
echo "完整的性能测试套件请使用: make test-performance"
echo ""

BASE_URL="${BASE_URL:-http://localhost:8080}"
DURATION="${DURATION:-30}"
CONCURRENCY="${CONCURRENCY:-50}"

echo "快速负载测试"
echo "============="
echo "目标: $BASE_URL"
echo "时长: ${DURATION}s"
echo "并发: $CONCURRENCY"
echo ""

# 检查 hey 是否安装
if ! command -v hey &> /dev/null; then
    echo "错误: 'hey' 未安装"
    echo "安装命令: go install github.com/rakyll/hey@latest"
    exit 1
fi

# 检查服务是否运行
if ! curl -s "$BASE_URL/health" > /dev/null; then
    echo "错误: 服务未运行或不可访问"
    echo "请先启动服务: make run"
    exit 1
fi

# 创建请求体
cat > /tmp/quick_test_request.json <<EOF
{
  "services": ["payment"],
  "context": {
    "user_id": "quick_test_user"
  }
}
EOF

echo "运行快速负载测试..."
hey -z ${DURATION}s -c ${CONCURRENCY} \
  -m POST \
  -H "Content-Type: application/json" \
  -D /tmp/quick_test_request.json \
  "$BASE_URL/experiment"

# 清理
rm -f /tmp/quick_test_request.json

echo ""
echo "快速测试完成！"
echo "查看指标: $BASE_URL/metrics"
echo "运行完整测试: make test-performance"
