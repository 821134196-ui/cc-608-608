#!/usr/bin/env bash
# 端到端冒烟测试：验证完整流程 + 三大关键不变量
set -euo pipefail
BASE=http://localhost:8080/api
J() { python3 -c "import sys,json; d=json.load(sys.stdin); print(eval(\"d$1\"))" 2>/dev/null; }

echo "== 1. 创建订单 =="
ORDER=$(curl -s -X POST $BASE/orders -H 'content-type: application/json' -d '{"customer_name":"李女士","phone":"13911112222","address":"阳光花园 3-502","hood_model":"老板 CXW-260","appointment_time":"2026-10-06 09:00","base_price_cents":26000}')
ID=$(echo "$ORDER" | J "['id']")
echo "订单 ID=$ID, 状态=$(echo "$ORDER" | J "['status']")"

echo "== 2. 师傅到场确认范围（型号不符，如实记录）=="
curl -s -X POST $BASE/orders/$ID/confirm-scope -H 'content-type: application/json' \
  -d '{"confirmed_model":"老板 CXW-260-27A3H","base_price_cents":26000,"scope_note":"实际型号与预约不符，为 27A3H，按标准范围清洗"}' | J "['status']"

echo "== 3. 开工 + 登记零件（油网复装，风轮故意不复装）=="
curl -s -X POST $BASE/orders/$ID/start | J "['status']"
P1=$(curl -s -X POST $BASE/orders/$ID/parts -H 'content-type: application/json' -d '{"name":"油网","quantity":1}' | J "['parts'][0]['id']")
P2=$(curl -s -X POST $BASE/orders/$ID/parts -H 'content-type: application/json' -d '{"name":"风轮","quantity":1}' | J "['parts'][1]['id']")
curl -s -X POST $BASE/orders/$ID/parts/$P1/reinstall >/dev/null
echo "零件 $P1 已复装, 零件 $P2 未复装"

echo "== 4. 上传前后照片 + 测量风量 =="
printf '\x89PNG\r\n\x1a\n' > /tmp/t.png
curl -s -X POST "$BASE/orders/$ID/photos?kind=before" -F "file=@/tmp/t.png;type=image/png" >/dev/null
curl -s -X POST "$BASE/orders/$ID/photos?kind=after" -F "file=@/tmp/t.png;type=image/png" >/dev/null
curl -s -X POST $BASE/orders/$ID/measurements/capture -H 'content-type: application/json' -d '{"phase":"before"}' >/dev/null
AFTER=$(curl -s -X POST $BASE/orders/$ID/measurements/capture -H 'content-type: application/json' -d '{"phase":"after"}')
echo "清洗前风量: $(echo "$AFTER" | J "['measurements'][0]['airflow_m3_min']")  清洗后: $(echo "$AFTER" | J "['measurements'][1]['airflow_m3_min']")"

echo "== 5. 加价项目：同意1 / 拒绝1 / 不处理1 =="
E1=$(curl -s -X POST $BASE/orders/$ID/extras -H 'content-type: application/json' -d '{"description":"更换止回阀","price_cents":8000}' | J "['extras'][0]['id']")
E2=$(curl -s -X POST $BASE/orders/$ID/extras -H 'content-type: application/json' -d '{"description":"加装防烟宝","price_cents":5000}' | J "['extras'][1]['id']")
E3=$(curl -s -X POST $BASE/orders/$ID/extras -H 'content-type: application/json' -d '{"description":"深度除味","price_cents":3000}' | J "['extras'][2]['id']")
curl -s -X POST $BASE/orders/$ID/extras/$E1/approve >/dev/null
curl -s -X POST $BASE/orders/$ID/extras/$E2/reject >/dev/null
BILL=$(curl -s $BASE/orders/$ID | J "['bill']['total_cents']")
echo "账单总额（应=26000+8000=34000）: $BILL"
[ "$BILL" = "34000" ] && echo "✅ 未确认/已拒绝的加价未计入账单" || { echo "❌ 账单金额错误"; exit 1; }

echo "== 6. 完工，但风轮未复装 → 验收应被阻止 =="
curl -s -X POST $BASE/orders/$ID/complete | J "['status']"
CODE=$(curl -s -o /tmp/acc.json -w "%{http_code}" -X POST $BASE/orders/$ID/accept -H 'content-type: application/json' -d '{"idempotency_key":"smoke-acc-1"}')
echo "缺件验收 HTTP $CODE: $(cat /tmp/acc.json | J "['error']")"
[ "$CODE" = "409" ] && echo "✅ 缺件成功阻止验收" || { echo "❌ 缺件未被阻止"; exit 1; }

echo "== 7. 复装风轮后验收（同一幂等键重试）=="
curl -s -X POST $BASE/orders/$ID/parts/$P2/reinstall >/dev/null
A1=$(curl -s -X POST $BASE/orders/$ID/accept -H 'content-type: application/json' -d '{"idempotency_key":"smoke-acc-1"}')
A2=$(curl -s -X POST $BASE/orders/$ID/accept -H 'content-type: application/json' -d '{"idempotency_key":"smoke-acc-1"}')
ACC1=$(echo "$A1" | J "['acceptance']['id']"); ACC2=$(echo "$A2" | J "['acceptance']['id']")
echo "两次验收记录 ID: $ACC1 / $ACC2"
[ "$ACC1" = "$ACC2" ] && echo "✅ 重复验收幂等（同一记录）" || { echo "❌ 重复验收产生多条记录"; exit 1; }
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/orders/$ID/accept -H 'content-type: application/json' -d '{"idempotency_key":"different-key"}')
[ "$CODE" = "409" ] && echo "✅ 不同幂等键的重复验收被拒绝(409)" || { echo "❌ 换键重复验收未被拒绝"; exit 1; }

echo "== 8. 验收后尝试追加加价 → 应被拒绝 =="
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/orders/$ID/extras -H 'content-type: application/json' -d '{"description":"事后加价","price_cents":9900}')
[ "$CODE" = "409" ] && echo "✅ 验收后加价被冻结" || { echo "❌ 验收后仍可加价"; exit 1; }
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/orders/$ID/extras/$E3/approve)
[ "$CODE" = "409" ] && echo "✅ 验收后待确认项目不能再生效" || { echo "❌ 验收后待确认项目仍可生效"; exit 1; }

echo "== 9. 收款：同一幂等键两次 + 不同键一次 =="
PAY1=$(curl -s -X POST $BASE/orders/$ID/pay -H 'content-type: application/json' -d '{"idempotency_key":"smoke-pay-1"}')
PAY2=$(curl -s -X POST $BASE/orders/$ID/pay -H 'content-type: application/json' -d '{"idempotency_key":"smoke-pay-1"}')
AMT1=$(echo "$PAY1" | J "['payment']['amount_cents']"); PID1=$(echo "$PAY1" | J "['payment']['id']"); PID2=$(echo "$PAY2" | J "['payment']['id']")
echo "收款金额: $AMT1, 两次收款记录 ID: $PID1 / $PID2"
[ "$AMT1" = "34000" ] && [ "$PID1" = "$PID2" ] && echo "✅ 重复收款幂等，金额=验收冻结额 34000" || { echo "❌ 收款幂等失败"; exit 1; }
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/orders/$ID/pay -H 'content-type: application/json' -d '{"idempotency_key":"pay-again"}')
[ "$CODE" = "409" ] && echo "✅ 不同幂等键的重复收款被拒绝(409)" || { echo "❌ 换键重复收款未被拒绝"; exit 1; }

echo ""
echo "🎉 全部冒烟检查通过：未确认加价不生效 / 缺件阻止验收 / 重复提交只办理一次"
