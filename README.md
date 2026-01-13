# yured

みんなで [KusaReMKN/yure](https://github.com/KusaReMKN/yure) にデータを送りつけて揺れを公開しましょう

## 使い方

IMU デバイスを見つけ、設定を行うため [libiio](https://github.com/analogdevicesinc/libiio) と適切な権限が必要です。めんどくさい場合は root で使いましょう（カス）。また、権限がない場合自動で権限昇格を行います。多分 libiio が動く環境ならどこでも動くはずです。

```bash
Usage: yured [OPTIONS]

Options:
  -b, --batch <BATCH>          [default: 30]
  -r, --rate <RATE>            [default: 100]
  -a, --algorithm <ALGORITHM>  [default: madgwick] [possible values: madgwick, mahony, vqf]
  -v, --verbose
  -h, --help                   Print help
```
