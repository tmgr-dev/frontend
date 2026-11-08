---
title: AI-агенты
section: Руководства
order: 10
description: Как подключить к TMGR Claude Code, Codex, Cursor и других MCP-агентов.
---

К TMGR можно подключить Claude Code, Codex, Cursor и любого другого агента с поддержкой MCP. Для этого есть два MCP-сервера: один работает с вашими задачами, другой зовёт вас, когда агенту нужно ваше внимание.

<!-- component:AiAgentsOverview -->

Хотите, чтобы агент работал в одном воркспейсе от имени персоны? Создайте токен персоны: Settings → Personas → Connect an agent.

## 1. Получите токен {#get-a-token}

Откройте [Settings → Agent alerts](/settings/agent-notifications) и нажмите Create token. Токен начинается с `tmgrn_` и показывается только один раз, поэтому скопируйте его сразу.

## 2. Подключите агента {#connect-your-agent}

Выберите клиент ниже и подставьте свой токен вместо заглушки. Нужен Node 22+, а для пушей и будильников ещё и мобильное приложение TMGR.

<!-- component:AgentConnectTabs -->

## Task MCP (tmgr) {#task-mcp}

Удалённый HTTP MCP-сервер для работы с задачами, статусами, комментариями, учётом времени и рутинами. Создайте токен устройства в [Settings → Smart devices](/settings?tab=device) и передавайте его в заголовке `X-Smart-Device-Token`.

### Claude Code {#task-mcp-claude-code}

```sh
claude mcp add --transport http tmgr {{mcpUrl}} \
  --header "X-Smart-Device-Token: <your device token>"
```

### mcp.json {#task-mcp-json}

```json
{
  "mcpServers": {
    "tmgr": {
      "type": "http",
      "url": "{{mcpUrl}}",
      "headers": {
        "X-Smart-Device-Token": "<your device token>"
      }
    }
  }
}
```

Это не OAuth-коннектор, поэтому пользовательские коннекторы claude.ai, которым нужен OAuth, не поддерживаются.

## Будильники {#alarms}

Инструмент `alarm` сначала звонит в iOS-приложении, а если сигнал не подтвердили, звонит вам на телефон (нажмите 1, чтобы подтвердить). Чтобы будильник срабатывал надёжно:

- Укажите и подтвердите номер для будильников в Settings → Agent alerts.
- Добавьте номер, с которого звонит TMGR, в контакты или в Избранное.
- Включите «Повторные вызовы» в настройках iOS.
- Разрешите будильники для приложения TMGR.

## Если что-то не работает {#troubleshooting}

- Не приходят пуши: проверьте, что файл `~/.config/tmgr-notify/env` существует и доступен для чтения. Хуки всегда завершаются с кодом 0 и ничего не выводят.
- Выполните `claude mcp list`, чтобы проверить, что сервер подключён.
- MCP-серверы не прописываются в `settings.json`, для них есть `claude mcp add`.
- Codex не читает `notify` из `.codex/config.toml` в папке проекта; укажите его в `~/.codex/config.toml`.
- Лимит: 20 запросов в минуту на один токен.

## Ссылки {#links}

- [@tmgr/notify на npm](https://www.npmjs.com/package/@tmgr/notify)
- [tmgr-notify на GitHub](https://github.com/tmgr-dev/tmgr-notify)

## Плагин руками агента {#plugins}

Попросите агента сделать плагин для десктопного приложения TMGR. Дайте ему [руководство по плагинам](/docs/plugins.md) или `/llms.txt`: там описаны манифест, разрешения и весь цикл «написать, проверить, попробовать».
