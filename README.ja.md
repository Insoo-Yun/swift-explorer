# Swift Explorer

[English](README.md) · [Türkçe](README.tr.md) · [简体中文](README.zh-CN.md)

Windows 11 のエクスプローラーと同じ見た目・同じ操作のファイル マネージャーです。Rust 製で、Windows と Linux で動きます。フォルダーはバックグラウンドのスレッドで読み込むので、大きなフォルダーを開いてもウィンドウが固まりません。検索はサブフォルダーを並列で調べます。

![日本語の画面](docs/japanese.png)

## ダウンロード

[Releases](../../releases) ページから `SwiftExplorer.exe` をダウンロードしてください。exe 1 つだけで、インストールは不要です。Windows 11 向けに作っています。Windows 10 でも動くはずですが、まだ試していません。

[Scoop](https://scoop.sh) の場合:

```powershell
scoop bucket add misutesu https://github.com/misutesu-desu/scoop-bucket
scoop install swift-explorer
```

Linux でのインストール方法は[英語の README](README.md#download) にあります。

## できること

- エクスプローラーと同じレイアウト: タイトル バーのタブ、パンくずリスト付きのアドレス バー、コマンド バー、ナビゲーション ウィンドウ、ホームと PC のページ。
- アイコン、サムネイル、ファイルの種類名は Windows のものをそのまま使います。
- エクスプローラーのショートカットはすべて同じように使えます。F1 で一覧を表示します。
- 検索ボックスに入力すると今のフォルダーをすぐに絞り込みます。Enter ですべてのサブフォルダーを検索し、`*` と `?` のワイルドカードも使えます。
- アドレス バーに `cmd`、`powershell`、`pwsh`、`wt` と入力すると、今のフォルダーでそのターミナルが開きます。
- コピー、切り取り、貼り付け、ドラッグ アンド ドロップはエクスプローラーやデスクトップとの間でも使えます。
- Shift + 右クリックで Windows の完全なコンテキスト メニューが開くので、7-Zip や「送る」もそのまま使えます。
- Ctrl+Z で名前の変更、移動、コピー、ごみ箱への削除を元に戻せます。
- 「表示 > テーマ」には、システム、ライト、ダークのほかに Catppuccin、Gruvbox、Everforest、Nord、Dracula、Tokyo Night、Rosé Pine、Solarized があります。
- コピーと移動は、ファイルがディスクに書き込まれるまで完了になりません。ターミナルで `sync` を実行するのと同じです。進行状況バーが消えたら USB メモリを抜いてかまいません。別のドライブへの移動では、元のファイルはそのあとで削除されます。
- 日本語・中国語・韓国語のファイル名はシステムの CJK フォントで表示されます。
- 表示言語: 日本語、English、Türkçe、Deutsch、Español、Français、Português (Brasil)、Русский、简体中文。Windows の表示言語に合わせて選ばれ、「表示 > 言語」で変更できます。

日本語訳はまだネイティブの方に確認してもらっていません。不自然なところがあれば [#7](../../issues/7) で教えてください。

## ビルド

Rust 1.82 以降が必要です。

```bash
cargo build --release
```

## ライセンス

[MIT](LICENSE)
