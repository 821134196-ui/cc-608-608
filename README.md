# 油烟机清洗验收系统

家庭厨房油烟机清洗的「施工—验收—结单」全流程系统。清洗门店向客户明示施工范围与结果，师傅逐件登记拆装零件，客户核对清洗前后对比、风量读数与复装清单后确认验收，验收通过才能结单。

## 一条命令启动

```bash
./start.sh
```

脚本会安装前端依赖（首次）、构建前端、编译并启动后端。打开 http://localhost:8080 即可使用。

- 后端：Rust + Axum，SQLx + SQLite（数据库文件 `hood.db`，自动建表）
- 前端：Svelte + Vite（构建产物由后端直接托管）
- 图片：保存到本地 `uploads/` 目录，经 `/uploads/...` 访问
- 测量仪：`GET /api/mock/meter/reading` 为模拟接口，施工中的「测量」按钮由后端调用同一模拟实现取数

环境变量（均有默认值，可不配置）：`DATABASE_URL`（默认 `sqlite://hood.db`）、`UPLOADS_DIR`（默认 `uploads`）、`FRONTEND_DIR`（默认 `frontend/dist`）、`BIND_ADDR`（默认 `0.0.0.0:8080`）。

## 业务流程

```
booked → confirmed → in_progress → awaiting_acceptance → accepted → closed
已预约   已确认范围   清洗中         待验收                 已验收    已结单
```

1. **客户预约**：填写型号、地址、预约时间并上传机器照片。
2. **师傅到场确认范围**：核对实际机型；型号不符时如实记录确认型号与备注。如需加价，提交「加价方案」——此时只是方案，不计入账单。
3. **施工**：逐件登记拆下的零件（油网、风轮…），上传清洗前/后照片，用模拟测量仪记录清洗前/后风量。
4. **完工提交**：要求前后照片、前后风量、零件登记齐全，否则无法完工。
5. **客户验收**：验收页并排展示清洗前后照片、风量对比、零件复装清单与费用明细；客户对加价项目逐项「同意/拒绝」。全部零件复装后才能确认验收。
6. **结单**：验收通过后才能收款，金额 = 验收时冻结的基础价 + 已同意加价。

## 关键不变量（有自动化测试覆盖）

| 规则 | 实现 |
| --- | --- |
| 未确认的加价不生效 | 账单只汇总 `approved` 的加价项目；`pending`/`rejected` 一律不计。验收后账单冻结，之后新增/同意加价均返回 409 |
| 客户拒绝后按原范围执行 | 拒绝的项目永不计费，订单按基础价继续验收结单 |
| 缺件不能通过验收 | 验收时校验所有登记零件已复装，缺件返回 409 并列出缺件数 |
| 验收/收款重复提交只办理一次 | 客户端为每次操作生成幂等键；`acceptances`/`payments` 表对 `order_id` 与 `idempotency_key` 双唯一约束，同事务内 `INSERT OR IGNORE` + 状态机校验。同键重放返回原记录，异键重复返回 409 |

## 测试

```bash
cargo test          # 10 个集成测试：三大不变量 + 主流程 + 状态机约束
./scripts/smoke.sh  # 对运行中的服务做端到端冒烟（需先 ./start.sh 启动）
```

## API 一览

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| POST | `/api/orders` | 创建预约订单 |
| GET | `/api/orders` / `/api/orders/{id}` | 订单列表 / 详情（含账单与验收阻碍项） |
| POST | `/api/orders/{id}/photos?kind=booking\|before\|after` | 上传照片（multipart） |
| POST | `/api/orders/{id}/confirm-scope` | 师傅确认范围与价格 |
| POST | `/api/orders/{id}/start` | 开工 |
| POST | `/api/orders/{id}/parts` | 登记拆下零件 |
| POST | `/api/orders/{id}/parts/{pid}/reinstall` | 标记复装 |
| DELETE | `/api/orders/{id}/parts/{pid}` | 删除误登记零件 |
| POST | `/api/orders/{id}/measurements/capture` | 从模拟测量仪取数（before/after） |
| POST | `/api/orders/{id}/extras` | 提出加价方案 |
| POST | `/api/orders/{id}/extras/{eid}/approve` / `reject` | 客户同意 / 拒绝加价 |
| POST | `/api/orders/{id}/complete` | 完工提交 |
| POST | `/api/orders/{id}/accept` | 客户确认验收（需 `idempotency_key`） |
| POST | `/api/orders/{id}/pay` | 收款结单（需 `idempotency_key`） |
| GET | `/api/mock/meter/reading` | 模拟测量仪读数 |

## 目录结构

```
├── src/            # Rust 后端（lib.rs 组装、routes.rs 业务、meter.rs 模拟测量仪）
├── migrations/     # SQLite 建表语句（含幂等唯一约束）
├── tests/api.rs    # 集成测试
├── frontend/       # Svelte 前端（预约 / 订单列表 / 师傅端 / 客户验收端）
├── scripts/smoke.sh# 端到端冒烟脚本
└── start.sh        # 一条命令启动
```
