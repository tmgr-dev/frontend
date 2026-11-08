# TMGR terminology glossary

Every translator uses these terms for the listed concepts; the `note` column records decisions. Tone rules are in `README.md`.

| en | ru | es | zh-CN | pt-BR | de | fr | ja | ko | pl | note |
|---|---|---|---|---|---|---|---|---|---|---|
| TMGR | TMGR | TMGR | TMGR | TMGR | TMGR | TMGR | TMGR | TMGR | TMGR | Product name, never translated |
| task | задача | tarea | 任务 | tarefa | Aufgabe | tâche | タスク | 작업 | zadanie | ru/pl neuter-fem: ru f., pl n. |
| subtask | подзадача | subtarea | 子任务 | subtarefa | Unteraufgabe | sous-tâche | サブタスク | 하위 작업 | podzadanie | ru f., pl n. |
| workspace | рабочее пространство | espacio de trabajo | 工作区 | espaço de trabalho | Arbeitsbereich | espace de travail | ワークスペース | 워크스페이스 | przestrzeń robocza | ru n., pl f. |
| project (category) | проект | proyecto | 项目 | projeto | Projekt | projet | プロジェクト | 프로젝트 | projekt | UI calls categories "projects" |
| status | статус | estado | 状态 | status | Status | statut | ステータス | 상태 | status | ru m., pl m. |
| backlog | бэклог | pendientes | 待办 | backlog | Backlog | backlog | バックログ | 백로그 | backlog | es "pendientes" avoids the loanword |
| in progress | в работе | en curso | 进行中 | em andamento | In Bearbeitung | en cours | 進行中 | 진행 중 | w toku | |
| done / completed | выполнено | completada | 已完成 | concluída | Erledigt | terminée | 完了 | 완료 | ukończone | Adjective agrees with task gender (es/pt/fr f.); ru/pl neuter in status labels |
| archived | в архиве | archivada | 已归档 | arquivada | Archiviert | archivée | アーカイブ済み | 보관됨 | zarchiwizowane | |
| archive (verb) | архивировать | archivar | 归档 | arquivar | Archivieren | archiver | アーカイブする | 보관 | archiwizuj | Button: imperative/infinitive |
| restore | восстановить | restaurar | 恢复 | restaurar | Wiederherstellen | restaurer | 復元 | 복원 | przywróć | |
| trash | корзина | papelera | 回收站 | lixeira | Papierkorb | corbeille | ゴミ箱 | 휴지통 | kosz | |
| board | доска | tablero | 看板 | quadro | Board | tableau | ボード | 보드 | tablica | zh "看板" = kanban board; de keeps Board |
| list | список | lista | 列表 | lista | Liste | liste | リスト | 목록 | lista | View name; ko uses native 목록 |
| dashboard | дашборд | panel | 仪表盘 | painel | Dashboard | tableau de bord | ダッシュボード | 대시보드 | pulpit | ru loanword is standard |
| daily routine | ежедневная рутина | rutina diaria | 日常事项 | rotina diária | Tagesroutine | routine quotidienne | デイリールーティン | 데일리 루틴 | codzienna rutyna | |
| routine | рутина | rutina | 例行事项 | rotina | Routine | routine | ルーティン | 루틴 | rutyna | |
| recurring | повторяющаяся | recurrente | 重复 | recorrente | Wiederkehrend | récurrente | 繰り返し | 반복 | cykliczne | |
| page | страница | página | 页面 | página | Seite | page | ページ | 페이지 | strona | Wiki-like document, not a route |
| comment | комментарий | comentario | 评论 | comentário | Kommentar | commentaire | コメント | 댓글 | komentarz | ko "댓글" for thread comments |
| attachment | вложение | adjunto | 附件 | anexo | Anhang | pièce jointe | 添付ファイル | 첨부 파일 | załącznik | |
| file | файл | archivo | 文件 | arquivo | Datei | fichier | ファイル | 파일 | plik | |
| member | участник | miembro | 成员 | membro | Mitglied | membre | メンバー | 멤버 | członek | ru m., "участник" not "пользователь" |
| invite | пригласить | invitar | 邀请 | convidar | Einladen | inviter | 招待 | 초대 | zaproś | Noun: ru приглашение, es invitación, de Einladung |
| assignee | исполнитель | responsable | 负责人 | responsável | Zuständig | assigné | 担当者 | 담당자 | wykonawca | de adjective form used as label; fr "assigné" |
| persona | персона | persona | 角色 | persona | Persona | persona | ペルソナ | 페르소나 | persona | AI agent identity shown as an assignee |
| agent | агент | agente | 智能体 | agente | Agent | agent | エージェント | 에이전트 | agent | AI agent; zh "智能体" not "代理" |
| agent work | работа агента | trabajo del agente | 智能体运行 | execução do agente | Agent-Lauf | exécution de l'agent | エージェントの実行 | 에이전트 작업 | praca agenta | One recorded run of an agent on a task |
| skill | навык | habilidad | 技能 | habilidade | Skill | compétence | スキル | 스킬 | umiejętność | de keeps loanword Skill; ja/ko katakana |
| token | токен | token | 令牌 | token | Token | jeton | トークン | 토큰 | token | API access token; fr "jeton". LLM tokens: keep as `token` |
| timer | таймер | temporizador | 计时器 | cronômetro | Timer | minuteur | タイマー | 타이머 | timer | Tracks a human's time on a task |
| time tracking | учёт времени | control de tiempo | 工时跟踪 | controle de tempo | Zeiterfassung | suivi du temps | 時間管理 | 시간 추적 | śledzenie czasu | |
| estimate | оценка | estimación | 预估 | estimativa | Schätzung | estimation | 見積もり | 예상 시간 | szacunek | ko "예상 시간" = estimated time |
| deadline | дедлайн | fecha límite | 截止时间 | prazo | Frist | échéance | 期限 | 마감 | termin | ru loanword is standard in task UIs |
| due date | срок | vencimiento | 到期日 | data de entrega | Fälligkeitsdatum | date d'échéance | 期日 | 마감일 | termin wykonania | |
| today | сегодня | hoy | 今天 | hoje | Heute | aujourd'hui | 今日 | 오늘 | dziś | |
| priority | приоритет | prioridad | 优先级 | prioridade | Priorität | priorité | 優先度 | 우선순위 | priorytet | |
| tag / label | метка | etiqueta | 标签 | etiqueta | Label | étiquette | ラベル | 라벨 | etykieta | Same term for tag and label |
| notification | уведомление | notificación | 通知 | notificação | Benachrichtigung | notification | 通知 | 알림 | powiadomienie | |
| settings | настройки | ajustes | 设置 | configurações | Einstellungen | paramètres | 設定 | 설정 | ustawienia | |
| profile | профиль | perfil | 个人资料 | perfil | Profil | profil | プロフィール | 프로필 | profil | |
| theme | тема | tema | 主题 | tema | Design | thème | テーマ | 테마 | motyw | de "Design" in UI context, not "Thema" |
| plugin | плагин | complemento | 插件 | plugin | Plugin | plugin | プラグイン | 플러그인 | wtyczka | es "complemento" |
| module | модуль | módulo | 模块 | módulo | Modul | module | モジュール | 모듈 | moduł | Feature module toggled per workspace |
| runner | раннер | runner | 运行器 | runner | Runner | runner | ランナー | 러너 | runner | Local agent runner; keep loanword where marked, zh "运行器" |
| session | сессия | sesión | 会话 | sessão | Sitzung | session | セッション | 세션 | sesja | |
| decision | решение | decisión | 决策 | decisão | Entscheidung | décision | 判断 | 결정 | decyzja | Agent asks the owner a question; ja "判断" fits "needs your call" |
| share link | ссылка для доступа | enlace para compartir | 分享链接 | link de compartilhamento | Freigabelink | lien de partage | 共有リンク | 공유 링크 | link udostępniania | |
| sign in | войти | iniciar sesión | 登录 | entrar | Anmelden | se connecter | ログイン | 로그인 | zaloguj się | Button label; pt-BR "Entrar" |
| sign out | выйти | cerrar sesión | 退出登录 | sair | Abmelden | se déconnecter | ログアウト | 로그아웃 | wyloguj się | |
| sign up | зарегистрироваться | registrarse | 注册 | criar conta | Registrieren | s'inscrire | 新規登録 | 가입하기 | zarejestruj się | |
| password | пароль | contraseña | 密码 | senha | Passwort | mot de passe | パスワード | 비밀번호 | hasło | |
| email | email | correo electrónico | 邮箱 | e-mail | E-Mail | e-mail | メールアドレス | 이메일 | e-mail | ru lowercase "email"; ja means the address |
| search | поиск | buscar | 搜索 | buscar | Suchen | rechercher | 検索 | 검색 | szukaj | Noun in headings, verb on buttons where the language differs |
| filter | фильтр | filtro | 筛选 | filtro | Filter | filtre | フィルター | 필터 | filtr | zh "筛选" |
| sort | сортировка | ordenar | 排序 | ordenar | Sortieren | trier | 並べ替え | 정렬 | sortuj | |
| drag | перетащить | arrastrar | 拖动 | arrastar | Ziehen | glisser | ドラッグ | 드래그 | przeciągnij | |
| duplicate | дублировать | duplicar | 复制 | duplicar | Duplizieren | dupliquer | 複製 | 복제 | duplikuj | zh "复制" is also copy; use "创建副本" when ambiguous |
| offline | офлайн | sin conexión | 离线 | offline | Offline | hors ligne | オフライン | 오프라인 | offline | |
| sync | синхронизация | sincronización | 同步 | sincronização | Synchronisierung | synchronisation | 同期 | 동기화 | synchronizacja | |
| desktop app | приложение для компьютера | aplicación de escritorio | 桌面应用 | aplicativo para desktop | Desktop-App | application de bureau | デスクトップアプリ | 데스크톱 앱 | aplikacja desktopowa | pt-BR "aplicativo" for apps |
| CLI | CLI | CLI | CLI | CLI | CLI | CLI | CLI | CLI | CLI | Keep in English |
| MCP | MCP | MCP | MCP | MCP | MCP | MCP | MCP | MCP | MCP | Keep in English |
| API | API | API | API | API | API | API | API | API | API | Keep in English |
