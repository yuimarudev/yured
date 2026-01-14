# yured

みんなで [KusaReMKN/yure](https://github.com/KusaReMKN/yure) にデータを送りつけて揺れを公開しましょう

## 使い方

[ビルド一覧](https://github.com/yuimarudev/yured/actions/workflows/cross-build.yml) の最新のビルド結果からビルド済みバイナリを入手するか、Cargo を使いビルドしてください。

IMU デバイスを見つけ、設定を行うため [libiio](https://github.com/analogdevicesinc/libiio) と適切な権限が必要です。めんどくさい場合は root で使いましょう（カス）。また、権限がない場合自動で権限昇格を行います。多分 libiio (とその依存関係) が入って動く環境ならどこでも動くはずです。

```bash
Usage: yured [OPTIONS]

Options:
  -b, --batch <BATCH>          [default: 30]
  -r, --rate <RATE>            [default: 100]
  -a, --algorithm <ALGORITHM>  [default: madgwick] [possible values: madgwick, mahony, vqf]
  -v, --verbose
  -h, --help                   Print help
```

[systemd service](./assets/etc/systemd/system/yured.service) もあります。[必要なドライバ](./assets/etc/modules-load.d/iio.conf) が入っていて、正しく設定すると簡単かつ勝手に起動してくれるのでおすすめです。assets ディレクトリにあるファイルをそれぞれ適切なパスに読み替え、内容を書き換えたあと `/etc/systemd/system/*` と `/etc/modules-load.d/*` にコピーし、`systemctl daemon-reload` と `systemctl enable yured.service sys-kernel-config.mount` をして再起動するとすべてが正常に働きます。(Linux / systemd 環境のみ)
