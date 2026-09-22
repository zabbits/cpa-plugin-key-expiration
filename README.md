# CPA Key Expiration

为 CPA 的 API Key 设置到期时间或禁用，在 CPAMP 中管理。

## 安装

解压对应平台的发布包，将动态库放入 CPA 的 `plugins` 目录。在 CPA 配置中启用插件：

```yaml
plugins:
  enabled: true
  dir: ./plugins
  configs:
    key-expiration:
      enabled: true
      database_path: ./plugins/key-expiration.sqlite3
```

重启 CPA，在 CPAMP 插件管理中打开「Key 有效期」。安装或升级后，打开一次页面同步 Key。

## 使用

- 点击「设置有效期」选择到期时间，勾选「永不过期」可取消时间限制。
- 点击「禁用」立即限制后续模型调用；重新启用已过期的 Key，还需调整有效期。
- 新增、删除 Key 使用 CPAMP 原有页面，随后刷新「Key 有效期」。删除后重新添加同一 Key 会保留原规则。

页面复用同源 CPAMP 已保存的登录凭据；未保存凭据或单独打开页面时，输入管理密钥即可。插件配置无需填写管理密钥。

时间按浏览器本地时区显示。规则保存在 `database_path`，容器部署时需持久化该文件所在目录。

限制作用于后续模型执行，不中断已开始的请求；`/v1/models` 等非模型执行接口仍使用 CPA 原有认证。

## 构建

GitHub Actions 的 [Linux build and release](https://github.com/zabbits/cpa-plugin-key-expiration/actions/workflows/ci.yml) 会在推送代码、创建或更新 PR 时自动运行，也可手动触发。

流程分别使用 Ubuntu 22.04 的 AMD64、ARM64 原生运行器，完成 Rust 格式检查、Clippy、单元测试和前端脚本语法检查后，编译并上传以下产物：

| 架构 | Actions artifact | 压缩包 |
| --- | --- | --- |
| AMD64 | `key-expiration-linux-amd64` | `key-expiration_<版本>_linux_amd64.zip` |
| ARM64 | `key-expiration-linux-arm64` | `key-expiration_<版本>_linux_arm64.zip` |

在成功运行页面的 **Artifacts** 中下载对应架构的产物，解压后可获得插件 ZIP 和 `.zip.sha256` 校验文件。插件 ZIP 内包含 `key-expiration.so`，产物保留 30 天。

```sh
sha256sum --check key-expiration_*.zip.sha256
```

本地构建需要 Rust 1.85+、Python 3.11+，执行 `make build` 后在 `dist/` 获取当前平台的动态库、ZIP 和 SHA256 校验文件。执行 `make check` 可运行代码检查和测试，需另外安装 Node.js。

## 发布 Release

将 `Cargo.toml` 和 `Cargo.lock` 中的项目版本更新并提交后，推送对应的 `v<版本>` 标签即可发布。例如当前版本为 `0.1.0`：

```sh
git tag v0.1.0
git push origin v0.1.0
```

工作流会校验标签与 `Cargo.toml` 的版本一致，等待 AMD64、ARM64 两种构建均成功，再创建 [GitHub Release](https://github.com/zabbits/cpa-plugin-key-expiration/releases)，自动生成发布说明，并上传两个 ZIP 和各自的 SHA256 校验文件。带预发布后缀的版本（例如 `v0.2.0-rc.1`）会标记为 Pre-release。

也可对已有版本标签手动运行工作流（使用 `workflow_dispatch` 的 `ref` 指定标签），或重新运行该标签的工作流。已有 Release 会更新对应附件。发布使用内置的 `GITHUB_TOKEN`，无需额外配置 Secret；普通分支和 PR 只构建，不发布 Release。
