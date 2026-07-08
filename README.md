# brecoder / 大猪啾录播机

这是一个功能简单的 bilibili 录播机, 只有 **录播** 和 **上传** 两项功能, 最初开发是用来录播[猪啾](https://live.bilibili.com/22391136)的


## 使用
1. 首先准备一份 `cookies.json` 文件, 若您是 biliup 用户, 可以直接在 `biliup/data/` 目录下发现以 uid 为开头的 .json 文件, 将其拷贝到同级目录下并改名为 `cookies.json` 即可
2. 配置要录制的直播间, 查看 config.json 文件, 然后在 `streamers` 中按照样例来进行配置
3. 启动大猪啾录播机

## 开发
该项目以纯 rust 开发(至少目前是, 不排除未来引入前端的可能性) 所以你只要愉快的 cargo run 就可以把项目跑起来啦

## 致谢
1. ~~[biliup-rs](https://github.com/biliup/biliup-rs)~~ 在该项目停止维护之后直接使用 [biliup](https://github.com/biliup/biliup) 中的部分代码, 见 `crates/biliup`

