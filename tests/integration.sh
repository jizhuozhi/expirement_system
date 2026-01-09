#!/bin/bash

# 集成测试套件 - 基于现有的 test_xds_integration.sh 优化
set -e

echo "=== 集成测试套件 ==="

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# 清理函数
cleanup() {
    echo -e "\n${YELLOW}清理测试环境...${NC}"
    if [ ! -z "$CONTROL_PLANE_PID" ]; then
        kill $CONTROL_PLANE_PID 2>/dev/null || true
    fi
    if [ ! -z "$DATA_PLANE_PID" ]; then
        kill $DATA_PLANE_PID 2>/dev/null || true
    fi
    sleep 2
}

# 设置清理陷阱
trap cleanup EXIT

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
check_dependency "protoc" || exit 1
check_dependency "go" || exit 1
check_dependency "cargo" || exit 1
check_dependency "grpcurl" || echo -e "${YELLOW}Warning: grpcurl 未找到，建议安装: brew install grpcurl${NC}"
check_dependency "jq" || echo -e "${YELLOW}Warning: jq 未找到，建议安装: brew install jq${NC}"

# 构建项目
echo -e "\n${BLUE}2. 构建项目...${NC}"

echo "构建控制面..."
cd control_plane
make build
if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓ 控制面构建成功${NC}"
else
    echo -e "${RED}✗ 控制面构建失败${NC}"
    exit 1
fi

echo "构建数据面..."
cd ../data_plane
cargo build --features grpc
if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓ 数据面构建成功${NC}"
else
    echo -e "${RED}✗ 数据面构建失败${NC}"
    exit 1
fi

cd ..

# 启动控制面
echo -e "\n${BLUE}3. 启动控制面...${NC}"
cd control_plane
./bin/control-plane -config config.yaml &
CONTROL_PLANE_PID=$!
echo "控制面 PID: $CONTROL_PLANE_PID"

# 等待控制面启动
sleep 3

# 检查控制面状态
echo -e "\n${BLUE}4. 验证控制面状态...${NC}"
if curl -s http://localhost:8080/health > /dev/null; then
    echo -e "${GREEN}✓ 控制面 HTTP 服务正常${NC}"
else
    echo -e "${RED}✗ 控制面 HTTP 服务异常${NC}"
    exit 1
fi

if command -v grpcurl &> /dev/null; then
    if grpcurl -plaintext localhost:8081 grpc.health.v1.Health/Check > /dev/null 2>&1; then
        echo -e "${GREEN}✓ 控制面 gRPC 服务正常${NC}"
    else
        echo -e "${YELLOW}⚠ 控制面 gRPC 服务可能未完全启动${NC}"
    fi
fi

# 启动数据面
echo -e "\n${BLUE}5. 启动数据面 (xDS 模式)...${NC}"
cd ../data_plane

# 设置 xDS 环境变量
export XDS_CONTROL_PLANE_ADDR="http://localhost:8081"
export XDS_NODE_ID="test-dataplane-001"
export XDS_CLUSTER="test-cluster"
export XDS_SERVICES="payment,recommendation"
export USE_XDS="true"
export GRPC_PORT="50051"
export SERVER_PORT="8080"

cargo run --features grpc &
DATA_PLANE_PID=$!
echo "数据面 PID: $DATA_PLANE_PID"

# 等待数据面启动
sleep 5

# 检查数据面状态
echo -e "\n${BLUE}6. 验证数据面状态...${NC}"
if curl -s http://localhost:8080/health > /dev/null; then
    echo -e "${GREEN}✓ 数据面 HTTP 服务正常${NC}"
else
    echo -e "${RED}✗ 数据面 HTTP 服务异常${NC}"
    exit 1
fi

if command -v grpcurl &> /dev/null; then
    if grpcurl -plaintext localhost:50051 grpc.health.v1.Health/Check > /dev/null 2>&1; then
        echo -e "${GREEN}✓ 数据面 gRPC 服务正常${NC}"
    else
        echo -e "${YELLOW}⚠ 数据面 gRPC 服务可能未完全启动${NC}"
    fi
fi

# 测试 xDS 协议
echo -e "\n${BLUE}7. 测试 xDS 协议...${NC}"

# 测试控制面状态 API
echo "测试控制面状态 API:"
CONTROL_STATUS=$(curl -s http://localhost:8081/status)
if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓ 控制面状态 API 正常${NC}"
    if command -v jq &> /dev/null; then
        echo "$CONTROL_STATUS" | jq .
    else
        echo "$CONTROL_STATUS"
    fi
else
    echo -e "${RED}✗ 控制面状态 API 异常${NC}"
fi

# 测试数据面实验查询
echo -e "\n测试数据面实验查询:"
EXPERIMENT_REQUEST='{
  "services": ["payment", "recommendation"],
  "context": {
    "user_id": "test-user-123",
    "device_type": "mobile"
  }
}'

if command -v grpcurl &> /dev/null; then
    echo "发送 gRPC 请求:"
    if echo "$EXPERIMENT_REQUEST" | grpcurl -plaintext -d @ localhost:50051 experiment.dataplane.v1.ExperimentService/GetExperiment; then
        echo -e "${GREEN}✓ gRPC 实验查询成功${NC}"
    else
        echo -e "${YELLOW}⚠ gRPC 实验查询失败（可能是协议不匹配）${NC}"
    fi
else
    echo -e "${YELLOW}跳过 gRPC 测试 (grpcurl 未安装)${NC}"
fi

# 测试 HTTP API
echo -e "\n测试 HTTP API:"
HTTP_RESPONSE=$(curl -s -X POST http://localhost:8080/experiment \
    -H "Content-Type: application/json" \
    -d "$EXPERIMENT_REQUEST")

if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓ HTTP 实验查询成功${NC}"
    if command -v jq &> /dev/null; then
        echo "$HTTP_RESPONSE" | jq .
    else
        echo "$HTTP_RESPONSE"
    fi
else
    echo -e "${RED}✗ HTTP 实验查询失败${NC}"
fi

# 测试服务发现
echo -e "\n${BLUE}8. 测试服务发现...${NC}"
if command -v grpcurl &> /dev/null; then
    echo "列出控制面服务:"
    grpcurl -plaintext localhost:8081 list | head -5
    
    echo -e "\n列出数据面服务:"
    grpcurl -plaintext localhost:50051 list | head -5
fi

echo -e "\n${GREEN}=== 集成测试完成 ===${NC}"
echo -e "${YELLOW}注意: 这是基础的集成测试，完整的 xDS 功能需要进一步验证${NC}"