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

本地构建需要 Rust 1.85+、Python 3.11+，执行 `make build` 后在 `dist/` 获取当前平台的动态库、ZIP 和 SHA256 校验文件。执行 `make check` 可运行代码检查和测试，需另外安装 Node.js。
