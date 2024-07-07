# Bit Until Trash

一个多目标文件夹定时备份工具。

## 使用

打开 Actions 页面，下载最新发布的二进制文件。

## 软件配置

### 配置文件示例
```toml
[settings]
interval = 300
filename = "%name%-%timestamp%"
compression = "zstd"

[backup.save]
from = "/home/mcseekeri/.local/share/PrismLauncher/instances/FengServer/.minecraft/saves/"
dest = "./"

[backup.Server]
from = "/opt/MCSManager/data/InstanceData/"
dest = "./"
```
## 配置文件位置
but 将依次在 `/etc/but.conf` `$HOME/.config/but.conf` 和 `./but.conf` 三个位置寻找配置文件，优先级从高到低。

### 作为系统服务运行

将 `but.service` 文件复制到 `/etc/systemd/system/` 目录，并执行以下命令：

```bash
systemctl daemon-reload
systemctl enable --now but
```

配置文件假设 but 位于 `/usr/local/bin/but`，您可以修改 `ExecStart` 字段以匹配实际位置，也可以将 but 软链接到 `/usr/local/bin/` 目录。

> 如果启动出错，可以输入`systemctl status but`查看错误日志。

## 备份原理
限于技术原因，目前 but 不支持增量备份，每次备份都会是完整备份。
不过为了节约空间，当指定目录未发生变化时，but 不会重复备份。