<h1 align="center">
  <br>
  <a href="https://github.com/enbop/fungi"><img src="https://raw.githubusercontent.com/enbop/fungi/master/assets/FullLogo_Transparent_NoBuffer.png" alt="Fungi logo" title="Fungi logo" width="150"></a>
  <br>
  <br>
  Fungi
  <br>
</h1>

<p align="center" style="font-size: 1.2rem;">将多设备融合为统一系统</p>
<p align="center" style="font-size: 1rem;">打造无缝设备集成的分布式计算平台</p>
<p align="center" style="font-size: 0.9rem; color: #666;">文件传输 • 端口转发 • 远程执行（即将推出）</p>

<p align="center">
  <a href="../README.md">🇺🇸 English</a> •
  <a href="README_ja.md">🇯🇵 日本語</a>
</p>

<div align="center">
  <img src="../assets/fungi-home-file-transfer.png" alt="File Transfer Interface" width="250" style="margin: 10px; border-radius: 8px; box-shadow: 0 4px 8px rgba(0,0,0,0.2);">
  <img src="../assets/fungi-data-tunnel.png" alt="Data Tunnel Interface" width="250" style="margin: 10px; border-radius: 8px; box-shadow: 0 4px 8px rgba(0,0,0,0.2);">
</div>

<hr/>

## 什么是 Fungi？

Fungi 让您通过 P2P 连接安全地连接和管理多个设备。没有服务器能看到您的数据 - 所有内容都在您的设备之间进行端到端加密。

基于 Rust 构建（采用 [rust-libp2p](https://github.com/libp2p/rust-libp2p) 实现 P2P 通信）并配以 Flutter 跨平台用户界面。

### 功能特性

📁 **文件传输**
- 将远程文件夹挂载为本地驱动器（支持 FTP/WebDAV）

🔗 **端口转发** 

🔮 **远程执行** *（即将推出）*

## 工作原理

**本地网络**：设备通过 mDNS 自动发现彼此 - 无需设置。

**互联网连接**：尝试 NAT 打洞进行直接 P2P 连接。如果成功，数据直接在设备间流动；否则使用中继服务器。所有流量都是端到端加密的 - 中继服务器只能看到加密的数据包。默认情况下，我们提供了一个中继服务器。

## 下载
[获取最新版本](https://github.com/enbop/fungi/releases)：

提供两个版本：
- **fungi-cli**：面向终端用户的命令行界面
- **fungi-app**：带有 Flutter 用户界面的图形化界面

### 快速开始（fungi-app）

#### 文件传输示例

假设您有两个设备：`设备 A` 和 `设备 B`，您希望 `设备 A` 访问 `设备 B` 上的文件。

#### 步骤 1：启动并获取 PeerID
1. 在两个设备上都启动 `Fungi App`
2. 点击应用顶部的 `PeerID` 自动复制并保存它们

#### 步骤 2：配置设备 B（文件服务器）
1. 导航到 **File Transfer > File Server > Incoming Allowed Peers**
2. 将设备 A 的 `PeerID` 添加到允许列表中
3. 设置 **Shared Directory** 为要共享的文件夹（例如 `/tmp`）并启用 **File Server State**

#### 步骤 3：从设备 A 连接
1. 转到 **File Transfer > Remote File Access > Add Remote Device**
2. 添加设备 B 的 PeerID 并分配别名

#### 步骤 4：访问文件
使用任何 FTP 或 WebDAV 客户端访问远程文件访问地址。
*（macOS 和 Windows 内置文件管理器都可以将 WebDAV 挂载为驱动器）*

#### 端口转发示例

将设备 B 的端口转发到设备 A：

#### 步骤 1：设置（同上）
启动应用并在设备间交换 PeerID。

#### 步骤 2：配置设备 B（端口服务器）
1. 导航到 **Data Tunnel > Port Listening Rules**
2. 添加要转发的端口（例如 `8080`）

#### 步骤 3：配置设备 A（端口客户端）
1. 导航到 **Data Tunnel > Port Forwarding Rules**
2. 添加设备 B 的 PeerID 并设置端口映射（例如本地 `9090` → 远程 `8080`）

#### 步骤 4：访问服务
在设备 A 上访问 `localhost:9090` 来访问设备 B 的端口 `8080` 上的服务。

> **注意**：更便捷的 mDNS 本地设备发现功能即将推出。

## 从源码构建

所有平台都需要安装 Rust 和 Flutter。

### 构建 fungi-cli

只需运行：
```bash
cargo build --release --bin fungi
```
二进制文件将位于：
```
./target/release/fungi
```

### 构建 fungi-app

#### Ubuntu
```bash
sudo apt-get install -y clang cmake ninja-build pkg-config libgtk-3-dev

cd flutter_app
flutter build linux --release
```

#### macOS
```bash
cd flutter_app
flutter build macos --release
```

#### Windows

安装 aws-lc-rs [构建依赖](https://aws.github.io/aws-lc-rs/requirements/windows.html)

确保您至少安装了：C/C++ 编译器、CMake、NASM

```bash
cargo build --release -p rust_lib_fungi_app
flutter build windows --release
```

## 平台支持

| 平台 | 状态 |
|----------|--------|
| macOS    | ✅ 就绪 |
| Windows  | ✅ 就绪 |
| Linux    | ✅ 就绪 |
| Android  | 🚧 开发中 |
| iOS      | 🚧 开发中 |

## 贡献

我们欢迎所有贡献：
- 🐛 错误报告和修复
- ✨ 新功能
- 📖 文档
- 🎨 界面改进

## 许可证

Apache License 2.0 - 详见 [LICENSE](../LICENSE)。
