# 环境变量配置指南

## 概述

本项目使用 `.env` 文件管理敏感配置（如 API Key）。`.env` 文件**已被 `.gitignore` 忽略**，不会被提交到仓库。

## 快速开始

在项目根目录下创建 `.env` 文件，内容如下：

```env
# nh-viewer environment variables

# API key for nhentai (required)
NH_API_KEY=你的API密钥
```

## 变量说明

| 变量名 | 必填 | 说明 |
|--------|------|------|
| `NH_API_KEY` | 是 | nhentai API 密钥，用于认证请求 |

## 如何获取 API Key

1. 登录 [nhentai.net](https://nhentai.net)
2. 进入 **Settings** > **API Keys**
3. 创建或复制你的 API Key

## 安全提醒

- **永远不要**将包含真实密钥的 `.env` 文件提交到 Git
- **永远不要**在公开场合（Issue、PR、聊天记录）分享你的 API Key
- 如果意外泄露，请立即在 nhentai 设置中重新生成密钥