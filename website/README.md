# Сайт deBang Launcher

Статический лендинг проекта. Без рантайм-зависимостей и без CDN: собранный
CSS лежит в репозитории, поэтому сайт работает на GitHub Pages сам по себе.

```
website/
├── index.html            # разметка
├── src/input.css         # исходник стилей (Tailwind 4 + компоненты темы)
├── assets/
│   ├── styles.css        # собранный CSS (коммитится, сайт его и грузит)
│   ├── js/main.js        # интерактив без библиотек
│   └── img/              # скриншоты WebP + favicon
├── .nojekyll             # чтобы Pages не гонял Jekyll
└── package.json          # только для пересборки CSS
```

## Локальный просмотр

```bash
cd website
npm run dev            # http://localhost:4173
```

## Пересборка CSS

Правьте `src/input.css`, затем:

```bash
npm install            # только dev-зависимости Tailwind
npm run build          # пишет assets/styles.css (минифицированный)
```

`assets/styles.css` закоммичен намеренно: деплой на Pages не должен зависеть
от Node.

## Публикация

Сайт включается в Actions: `.github/workflows/pages.yml` публикует папку
`website/` в GitHub Pages на ветке `main`.

Вручную: **Settings → Pages → Source: GitHub Actions**.

Если включаете сайт не из корня репозитория, а из подпапки, все пути в
`index.html` относительные (`assets/…`) — менять ничего не нужно.

## Что где править

| Что | Где |
| --- | --- |
| Тексты, секции, ссылки | `index.html` |
| Цвета темы, стекло, градиенты, анимации | `website/src/input.css` (`:root`, `@theme`) |
| Меню, звёзды GitHub, версия, копирование команды, scroll-анимации | `assets/js/main.js` |
| Скриншоты | `assets/img/*.webp` |

Репозиторий задан одной строкой в `assets/js/main.js` (`const REPO`), скрипт
расставляет его по всем ссылкам и тянет счётчик звёзд и последний релиз через
GitHub API. Без сети остаются заглушки — страница остаётся рабочей.
