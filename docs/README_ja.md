<h1 align="center">
  <br>
  <a href="https://github.com/enbop/fungi"><img src="https://raw.githubusercontent.com/enbop/fungi/master/assets/FullLogo_Transparent_NoBuffer.png" alt="Fungi logo" title="Fungi logo" width="150"></a>
  <br>
  <br>
  Fungi
  <br>
</h1>

<p align="center" style="font-size: 1.2rem;">複数デバイスを統合システムに</p>
<p align="center" style="font-size: 1rem;">シームレスなマルチデバイス統合のためのプラットフォーム</p>
<p align="center" style="font-size: 0.9rem; color: #666;">ファイル転送 • ポートフォワーディング • クロスデバイス統合（近日公開）</p>

<p align="center">
  <a href="../README.md">🇺🇸 English</a> •
  <a href="README_zh.md">🇨🇳 简体中文</a>
</p>

<div align="center">
  <img src="../assets/fungi-home-file-transfer.png" alt="File Transfer Interface" width="250" style="margin: 10px; border-radius: 8px; box-shadow: 0 4px 8px rgba(0,0,0,0.2);">
  <img src="../assets/fungi-data-tunnel.png" alt="Data Tunnel Interface" width="250" style="margin: 10px; border-radius: 8px; box-shadow: 0 4px 8px rgba(0,0,0,0.2);">
</div>

<hr/>

## Fungiとは？

FungiはP2P接続を通じて複数のデバイスを安全に接続・管理できるツールです。サーバーがあなたのデータを見ることはありません - すべてデバイス間でエンドツーエンド暗号化されています。

Rust（P2P通信に[rust-libp2p](https://github.com/libp2p/rust-libp2p)を採用）をベースに構築し、Flutterによるクロスプラットフォーム対応のユーザーインターフェースを提供しています。

### 主要機能

📁 **ファイル転送**
- リモートフォルダをローカルドライブとしてマウント（FTP/WebDAV対応）

🔗 **ポートフォワーディング** 

🔮 **クロスデバイス統合** *（近日公開）*

## 動作原理

**ローカルネットワーク**：デバイスはmDNSを介して自動的にお互いを発見します - 設定不要。

**インターネット**：直接P2P接続のためのNATホールパンチングを試みます。成功すれば、データはデバイス間で直接流れます；そうでなければリレーサーバーを使用します。すべてのトラフィックはエンドツーエンド暗号化されており、リレーサーバーは暗号化されたデータパケットしか見ることができません。デフォルトで、リレーサーバーを提供しています。

## ダウンロード
[最新リリースを入手](https://github.com/enbop/fungi/releases)：

2つのバージョンを提供：
- **fungi-cli**：ターミナルユーザー向けのコマンドライン インターフェース
- **fungi-app**：FlutterUIを備えたグラフィカル ユーザー インターフェース

### クイックスタート（fungi-app）

#### ファイル転送の例

2つのデバイス：`デバイスA`と`デバイスB`があり、`デバイスA`で`デバイスB`のファイルにアクセスしたいとします。

#### ステップ1：起動とPeerIDの取得
1. 両方のデバイスで`Fungi App`を起動
2. アプリ上部の`PeerID`をクリックして自動的にコピーし、それらを保存

#### ステップ2：デバイスBの設定（ファイルサーバー）
1. **File Transfer > File Server > Incoming Allowed Peers**に移動
2. デバイスAの`PeerID`を許可リストに追加
3. **Shared Directory**を共有したいフォルダに設定（例：`/tmp`）し、**File Server State**を有効にする

#### ステップ3：デバイスAから接続
1. **File Transfer > Remote File Access > Add Remote Device**に移動
2. デバイスBのPeerIDを追加してエイリアスを割り当て

#### ステップ4：ファイルへのアクセス
任意のFTPまたはWebDAVクライアントを使用してリモートファイルアクセスアドレスにアクセス。
*（macOSとWindowsの内蔵ファイルマネージャーはWebDAVをドライブとしてマウントできます）*

#### ポートフォワーディングの例

デバイスBからデバイスAへポートを転送する場合：

#### ステップ1：セットアップ（上記と同じ）
アプリを起動し、デバイス間でPeerIDを交換。

#### ステップ2：デバイスBの設定（ポートサーバー）
1. **Data Tunnel > Port Listening Rules**に移動
2. 転送したいポートを追加（例：`8080`）

#### ステップ3：デバイスAの設定（ポートクライアント）
1. **Data Tunnel > Port Forwarding Rules**に移動
2. デバイスBのPeerIDを追加し、ポートマッピングを設定（例：ローカル`9090` → リモート`8080`）

#### ステップ4：サービスへのアクセス
デバイスAで`localhost:9090`にアクセスして、デバイスBのポート`8080`のサービスにアクセス。

> **注意**：より便利なmDNSローカルデバイス発見機能が近日公開予定です。

### クイックスタート（fungi-cli）

[CLI サービスクイックスタートガイド](cli_service_quick_start.md)をご覧ください。

## ソースからビルド

すべてのプラットフォームでRustとFlutterのインストールが必要です。

### fungi-cliのビルド

以下を実行するだけです：
```bash
cargo build --release --bin fungi
```
バイナリファイルは以下の場所にあります：
```
./target/release/fungi
```

### fungi-appのビルド

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

aws-lc-rs [ビルド依存関係](https://aws.github.io/aws-lc-rs/requirements/windows.html)をインストール

最低限以下が必要です：C/C++コンパイラ、CMake、NASM

```bash
cargo build --release -p rust_lib_fungi_app
flutter build windows --release
```

## プラットフォームサポート

| プラットフォーム | ステータス |
|----------|--------|
| macOS    | ✅ 対応済み |
| Windows  | ✅ 対応済み |
| Linux    | ✅ 対応済み |
| Android  | 🚧 開発中 |
| iOS      | 🚧 開発中 |

## 貢献

すべての貢献を歓迎します：
- 🐛 バグレポートと修正
- ✨ 新機能
- 📖 ドキュメント
- 🎨 UI改善

## ライセンス

Apache License 2.0 - 詳細は[LICENSE](../LICENSE)をご覧ください。
