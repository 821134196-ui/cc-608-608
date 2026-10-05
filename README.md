# 净烟工坊 · 家庭厨房油烟机清洗前后验收系统

清洗门店向客户说明实际施工范围与结果的验收系统：师傅逐件记录拆装零件，客户在完工页并排核对清洗前后照片、风量读数与复装清单后再确认验收，**只有客户确认验收才能结单收款**。

## 技术栈

| 层 | 技术 |
|---|---|
| 后端 | Rust + Axum 0.8 + Tokio |
| 数据库 | SQLite（SQLx 0.8，嵌入式迁移，运行时自动建表） |
| 前端 | Svelte 4 + Vite 5（构建后由 Axum 直接托管，SPA 回退） |
| 图片 | multipart 上传，落盘到本地 `data/uploads/`，静态目录 `/uploads` 访问 |
| 测量仪 | 模拟接口 `GET /api/orders/{id}/meter/{phase}`，读数稳定、洗后高于洗前 |

## 一条命令启动

```bash
./run.sh          # 生产模式：构建前端 → 启动 http://127.0.0.1:8080
./run.sh dev      # 开发模式：前端 5173 热更新 + 后端 8080
```

首次运行自动安装 Rust（rustup，若缺失）、`npm install`、前端构建、数据库迁移。
浏览器打开 <http://127.0.0.1:8080>，右上角可切换 **客户视图 / 师傅视图**（演示用身份切换）。

环境变量（均有默认值）：`PORT=8080`、`DATABASE_URL=data/hoodcare.db`、`UPLOADS_DIR=data/uploads`、`FRONTEND_DIST=frontend/dist`。

## 业务流程

```
客户预约(型号/照片/时间)
   └─ 师傅到场核对型号
        ├─ 型号一致 → 按原范围开工
        └─ 型号不符 → 自动生成新方案 → 客户决定
              ├─ 同意 → 按新方案开工
              └─ 拒绝 → 仍按原型号/原范围开工
施工中：逐件登记拆下零件 → 上传清洗前照片 → 测洗前风量
        需要增加收费 → 师傅只报价 → 客户确认 → 师傅施工 → 才计入账单
        复装零件逐件勾选 → 上传清洗后照片 → 测洗后风量
完工自检（缺件/照片/风量/待确认方案 任一不满足都挡回）
   └─ 客户在完工验收页并排核对：前/后照片 · 前/后风量 · 复装清单 · 费用明细
        ├─ 驳回 → 退回施工整改
        └─ 确认验收 → 结单收款（金额服务端重算）→ completed
```

## 三条争议防护规则（重点）

### 1. 未经客户确认的加价不会生效

- 师傅只能创建 `pending` 方案（`POST /addons`），完工提交后禁止再新增项目；
- 同类型同时只允许一个待确认方案（DB 部分唯一索引），决定一次生效不可更改；
- 未确认（`pending`）/ 已拒绝（`rejected`）的项目：**禁止标记施工**（409）、**禁止完工**、**不计入账单**；
- 即使客户已同意但师傅未标记 `applied`，也不计费；只有 `approved AND applied` 才进账单；
- 收款金额完全由服务端按生效项目重算，前端无法篡改。

### 2. 缺件阻止验收

- 零件按 `(订单, 名称)` 唯一登记，复装为幂等操作（重复点击不多记）；
- 完工提交与客户验收两道闸门都在**服务端**重新扫描“已拆未装”零件，缺件一票否决；
- 并要求前/后照片齐全、前/后风量齐全且洗后严格高于洗前。

### 3. 重复提交只办理一次、不重复收费

- 验收记录 `accepted=1` 全订单唯一（部分唯一索引 + 状态检查，事务内落库）；
- 收款用 `BEGIN IMMEDIATE` 事务串行化：进入即取写锁 → 查 payment 是否已存在 → 写 payment 与模拟扣款流水 `charge_events`；
- 5 个并发收款请求的实测结果：恰好 1 个 200、4 个 409，扣款流水仅 1 笔（见并发测试）。

## API 一览

| 方法 路径 | 说明 |
|---|---|
| `POST/GET /api/orders` | 客户预约 / 订单列表 |
| `GET /api/orders/{id}` | 订单聚合详情（账单明细、缺件清单、验收/收款状态） |
| `POST /api/orders/{id}/arrive` | 师傅到场核对型号（不符自动生成待确认方案） |
| `POST /api/orders/{id}/start-work` | 客户对型号方案决定后开工 |
| `POST/GET /api/orders/{id}/addons` | 提出加价方案 / 列表 |
| `POST /api/orders/{id}/addons/{aid}/decision` | 客户 `approved` / `rejected` |
| `POST /api/orders/{id}/addons/{aid}/apply` | 师傅确认按已同意方案施工 |
| `POST/GET /api/orders/{id}/parts` | 零件拆下登记 / 列表 |
| `POST /api/orders/{id}/parts/{pid}/reinstall` | 复装登记（幂等） |
| `POST/GET /api/orders/{id}/photos` | 照片 multipart 上传 / 列表 |
| `GET /api/orders/{id}/meter/{phase}` | 模拟测量仪实时读数 |
| `POST /api/orders/{id}/measurements` | 记录风量（按订单+阶段 UPSERT） |
| `POST /api/orders/{id}/complete-work` | 完工自检并提交客户验收 |
| `POST /api/orders/{id}/acceptance` | 客户验收（`accepted:true/false` 驳回） |
| `POST /api/orders/{id}/payment` | 结单收款（仅已验收，且全单仅一次） |

## 测试

```bash
cargo test
```

14 个端到端集成测试（Axum `oneshot` + 临时 SQLite/上传目录），覆盖：未确认/已拒绝/同意未施工加价均不计费、型号不符两条分支、缺件阻止完工与验收、验收/收款重复提交、**并发收款只扣一次**、复装幂等、风量 UPSERT、真实 multipart 图片落盘、状态机顺序约束。
