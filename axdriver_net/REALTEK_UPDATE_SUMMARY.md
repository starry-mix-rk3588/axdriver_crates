# Realtek 驱动更新总结

## 更新日期
2025年10月31日

## 更新内容

### 1. RTL8139 驱动初始化流程更新

根据 `realtek/rtl8139.c` 的参考实现，更新了初始化步骤顺序：

#### 初始化步骤（按正确顺序）：
1. **Power On** - 设置 CONFIG1 = 0x00
2. **Software Reset** - 执行软复位并等待完成
3. **分配 RX Buffer** - 分配接收缓冲区
4. **分配 TX Buffers** - 分配4个发送缓冲区
5. **启用 Tx/Rx** - 启用发送和接收（在配置之前）
6. **配置 TCR** - 传输配置寄存器 (DMA burst=1024, IFG=normal)
7. **配置 RCR** - 接收配置寄存器 (接受所有包 + WRAP + 最大DMA)
8. **设置 TSAD0-3** - 设置发送地址描述符
9. **设置 RBSTART** - 设置接收缓冲区起始地址
10. **初始化 MPC** - 清零丢包计数器
11. **配置中断** - 设置中断掩码 (ROK, TOK, RER, TER, RXOVW)
12. **读取 MAC 地址** - 从设备读取MAC地址

#### 关键差异：
- ✅ **增加了 CONFIG1 初始化**（之前缺失）
- ✅ **在配置寄存器之前先启用 Tx/Rx**（顺序修正）
- ✅ **显式初始化 MPC 寄存器**（之前缺失）
- ✅ **增加了更多中断类型 (RXOVW)**
- ✅ **RCR 配置更加完整**（增加了64K buffer设置）

### 2. RTL8169 驱动初始化流程更新

根据 `realtek/rtl8169.c` 的参考实现，更新了初始化步骤顺序：

#### 初始化步骤（按正确顺序）：
1. **Software Reset** - 执行软复位
2. **解锁配置寄存器** - CFG_9346 = 0xC0
3. **分配描述符环** - 分配 TX/RX 描述符
4. **设置 TX Ring** - 设置发送描述符和缓冲区
5. **设置 RX Ring** - 设置接收描述符和缓冲区
6. **配置 RCR** - 接收配置寄存器
7. **启用 TE** - 先只启用发送器
8. **配置 TCR** - 传输配置寄存器
9. **设置 RMS** - 最大接收包大小
10. **设置 ETTHR** - 早期传输阈值 (0x3B)
11. **设置 RDSAR** - RX 描述符地址（低32位和高32位）
12. **设置 TNPDS** - TX 描述符地址（低32位和高32位）
13. **配置中断** - 设置中断掩码（增加 LINKCHG）
14. **启用 RX/TX** - 启用接收和发送
15. **设置 MAR0/MAR4** - 多播过滤器（接受所有）
16. **锁定配置寄存器** - CFG_9346 = 0x00
17. **读取 MAC 地址** - 从设备读取MAC地址

#### 关键差异：
- ✅ **先启用 TE，再配置 TCR**（顺序修正）
- ✅ **显式设置 ETTHR 寄存器**（之前缺失）
- ✅ **分别设置描述符地址的高低32位**（更明确）
- ✅ **增加了 MAR0/MAR4 配置**（多播过滤器）
- ✅ **增加了 LINKCHG 中断**（链路状态变化）
- ✅ **配置寄存器的解锁/锁定位置更准确**

### 3. 寄存器定义增强

#### RTL8139 新增寄存器：
```rust
pub const MULINT: u16 = 0x5C;          // Multiple Interrupt Select
pub const INT_LENCHG: u16 = 1 << 13;   // Cable Length Change
pub const INT_TIMEOUT: u16 = 1 << 14;  // Time Out

// RCR/TCR 配置位
pub const RCR_MXDMA_SHIFT: u32 = 8;
pub const RCR_RBLEN_SHIFT: u32 = 11;
pub const RCR_RXFTH_SHIFT: u32 = 13;
pub const TCR_MXDMA_SHIFT: u32 = 8;
pub const TCR_IFG_SHIFT: u32 = 24;
```

#### RTL8169 新增寄存器：
```rust
pub const RCR_RXFTH_64: u32 = 2 << 13;    // 64字节阈值选项
pub const RCR_MERINT: u32 = 1 << 24;      // Multiple Early Interrupt
pub const TCR_LOOPBACK: u32 = 3 << 17;    // 回环模式
```

### 4. Debug 信息增强

为两个驱动都添加了详细的 debug 日志：

#### 初始化阶段：
- 每个初始化步骤都有明确的 log
- 显示寄存器地址和写入的值
- 验证关键寄存器的读取值

#### 运行时：
- TX：显示描述符索引、包长度、状态
- RX：显示描述符索引、包长度、状态、错误
- 内存分配：显示虚拟地址和物理地址
- 缓冲区管理：显示指针位置和回绕

#### 示例日志：
```
[RTL8139] Step 1: Power on device (CONFIG1 = 0x00)
[RTL8139] Step 2: Performing software reset
[RTL8139] Step 3: Allocating RX buffer (size: 10240)
[RTL8139] RX buffer allocated at vaddr=0x..., paddr=0x...
[RTL8139] Step 6: Configuring TCR = 0x3000600
[RTL8139] TCR register = 0x3000600
[RTL8139] TX desc[0]: len=60, TSD offset=0x10
```

### 5. 软件复位超时调整

#### RTL8139:
- 超时时间：100us * 1000 = 100ms
- 更符合硬件复位时间要求

#### RTL8169:
- 超时时间：10ms * 1000 = 10s
- 给予更长的复位时间（千兆网卡需要更长时间）

### 6. 数据传输改进

#### 错误处理：
- 添加了更详细的错误日志
- 区分不同的错误类型
- 显示描述符状态

#### 缓冲区管理：
- RX buffer 回绕检测和日志
- TX/RX 描述符索引追踪
- 内存分配失败的详细报告

## 测试建议

### 1. 初始化测试
```bash
# 查看初始化日志
dmesg | grep RTL8139
dmesg | grep RTL8169
```

### 2. 寄存器验证
检查日志中的寄存器值是否符合预期：
- CR (Command Register)
- TCR (Transmit Configuration)
- RCR (Receive Configuration)
- IMR (Interrupt Mask)

### 3. 数据传输测试
```bash
# Ping 测试
ping -c 10 <target>

# 大数据量测试
iperf3 -c <server>
```

### 4. 错误场景测试
- 网络断开重连
- 大包/小包混合传输
- 高负载场景

## 参考文件

- `realtek/rtl8139.c` - RTL8139 C 驱动参考实现
- `realtek/rtl8169.c` - RTL8169 C 驱动参考实现
- RTL8139 数据手册
- RTL8169/8168/8111 数据手册

## 注意事项

1. **寄存器访问顺序很重要** - 某些寄存器必须在特定状态下访问
2. **配置寄存器锁定** - RTL8169 需要解锁/锁定配置寄存器
3. **内存屏障** - 在更新描述符后需要内存屏障
4. **中断处理** - 需要正确清除中断标志

## 后续工作

1. [ ] 添加中断处理函数的 debug 信息
2. [ ] 实现更详细的统计信息收集
3. [ ] 添加性能监控和调优
4. [ ] 支持更多的硬件特性（校验和卸载等）
