# Configurações rápidas para o painel do COSMIC

Applet no estilo das configurações rápidas do Android 17, do GNOME e do Windows e da
Central de Controle do macOS e do iOS.
No painel ficam, da esquerda para a direita:

- **privacidade**, como no Android e no macOS: uma câmera verde quando um app usa a
  câmera, um microfone laranja quando um app grava do microfone (gravar o som do
  computador não conta); passar o mouse diz qual app;
- um balão de **notificações**: na cor de destaque com quantas chegaram desde a última vez
  que você abriu o applet; dentro, as novas têm um ponto;
- os ícones dos **blocos ligados** (Bluetooth, Não perturbe, Modo avião, Lado a lado…; dá
  para desligar isso na edição);
- rede e volume; rolar o mouse sobre o volume aumenta ou diminui, o botão do meio liga e
  desliga o mudo;
- as baterias: cada periférico sem fio (mouse, teclado, fone…) com a carga dele e, na
  borda, a do computador; rolar o mouse sobre ela muda o brilho da tela.

Num painel na lateral da tela, os ícones ficam em coluna, com os números (notificações
novas, bateria) embaixo de cada um.

Passar o mouse sobre cada ícone mostra o que ele diz: a rede, o volume, a bateria com o
tempo restante, cada bloco ligado. Clicar abre:

- bateria em %, com o tempo que falta quando o sistema sabe (clicar abre a página de
  Energia; num espaço curto, o tempo se abrevia em vez de empurrar os botões), e botões
  redondos de editar, Configurações e desligar (que abre Bloquear, Suspender, Reiniciar,
  Desligar e Sair, em blocos grandes); a captura de tela é um bloco que dá para adicionar;
- quem está usando a câmera ou o microfone, quando alguém está;
- sliders grossos de volume e de brilho, como no Android e no iOS: o ícone fica dentro,
  arrasta-se em qualquer ponto, a roda do mouse também muda; o ícone do volume liga e
  desliga o mudo (o slider fica cinza), e a seta ao lado abre a página; as pontas dos
  sliders e os botões redondos seguem o arredondamento escolhido no tema do COSMIC;
- blocos de Wi-Fi, Bluetooth (com a bateria do dispositivo conectado), Energia (alterna
  Economia/Equilibrado/Desempenho), Não perturbe, Tema escuro e Modo avião; dá para
  adicionar VPN (liga a última usada, ou desliga as que estão ligadas), Microfone mudo, Manter acordado, Lado a lado (tiling), Lupa, Inverter cores,
  Leitor de tela, Alto contraste, Captura de tela, Espaços de trabalho, Touchpad (liga e
  desliga o touchpad, sem mexer no resto da configuração dele), DNS (passa pelos servidores
  da rede, Cloudflare, Google, Quad9, AdGuard e os que você tinha posto à mão, na conexão em
  uso; a seta abre a lista) e Borda ativa (liga e desliga
  a [borda ativa](https://github.com/DrakRosmann/cosmic-ext-hot-edge); só aparece com ela
  instalada, e a seta abre o app dela). Os blocos acendem e apagam
  aos poucos e cedem um pouco ao serem apertados, como no Android; a seta redonda dentro
  deles, ou o clique direito em qualquer bloco (o toque longo do Android), abre a página;
  os pequenos, só com o ícone, dizem o nome ao passar o mouse;
- a seta do Lado a lado abre a página do tiling, como a do applet oficial: o espaço de
  trabalho atual, o comportamento dos espaços novos (Lado a lado ou Flutuante), os
  atalhos e Realçar janela ativa.

- **o que está tocando** (qualquer player MPRIS: Spotify, navegadores, players de vídeo),
  compacto como no macOS e no Android, na cor da capa: capa, título, artista e
  anterior/play/próxima numa linha, e o progresso (arrastável) embaixo; com mais de um
  player, "1/2 ›" passa para o próximo (um player que começa a tocar vem para a frente); o
  alto-falante escolhe a saída de som ali mesmo;
- **notificações** recebidas, num painel à parte, abaixo dos controles, como no Android
  (sem nenhuma, ele diz isso numa linha): das mais novas para as mais antigas, as de um
  mesmo app juntas (só a mais nova, com "+N" no cartão para ver o resto): clicar abre, os
  botões do app
  (como Responder ou Marcar como lida) aparecem no card, "×" dispensa, "Limpar tudo"
  limpa. O ícone é o do app, do arquivo `.desktop` dele, como no GNOME; o HTML que alguns
  apps mandam no texto (links, negrito) vira texto simples. Elas continuam lá depois de reiniciar a sessão.
  Fechar no "×" o aviso que aparece na tela não tira a notificação daqui, como no Android;
  clicar no aviso para abrir, sim. O applet se apresenta ao painel
  como applet de notificações (`X-NotificationsApplet`) e recebe as notificações do daemon
  do COSMIC, junto com o applet de notificações oficial.

A seta ">" abre uma página dentro do próprio painel, com "←" para voltar e um atalho para
o Configurações no rodapé:

- **Wi-Fi**: liga/desliga e lista as redes. Clicar numa rede salva ou aberta conecta; uma
  rede protegida nova abre o Configurações, que é quem pede a senha.
- **VPN**: as VPNs e as conexões WireGuard salvas no NetworkManager, a última usada
  primeiro; clicar liga ou desliga. Uma que pede senha abre o Configurações.
- **Bluetooth**: liga/desliga e lista os dispositivos pareados, com a bateria dos
  conectados; clicar conecta ou desconecta.
- **Energia**: a bateria do computador e de cada periférico, a opção de mostrar a
  porcentagem da bateria do computador na barra, e os três modos.
- **Som** (seta do volume): a saída de áudio (as portas sem nada ligado, como HDMI sem
  monitor, ficam de fora), o volume de cada app que está tocando, como no mixer do Windows
  (o ícone do app, dentro do slider, o deixa mudo), e a entrada: volume do microfone e qual
  microfone usar.
- **Não perturbe**: até você desligar, por 1 hora, por 3 horas ou até amanhã às 8h; o bloco
  mostra até quando.
- **Telas** (seta do brilho): cada monitor com resolução e taxa, brilho e escala
  (100–200 %). O brilho de monitores externos vem do `ddcutil`, que precisa de acesso aos
  dispositivos `/dev/i2c-*` (por exemplo, uma regra do udev dando acesso ao seu usuário).

As páginas entram deslizando (da direita ao entrar, da esquerda ao voltar), com fade. A
altura muda de uma vez: redimensionar o popup a cada quadro derrubava o renderizador. O
popup fica sob o applet, rente à ponta dele (num canto da tela, quase encostado na
borda), e vai até quase a altura da tela; o que passar disso rola, as notificações embaixo
dos controles.

O lápis no topo abre a edição, como no Android: os blocos aparecem com "−" para tirar;
arraste um bloco para outra posição (os outros abrem espaço enquanto ele passa); tocar
num bloco o seleciona para mover (Antes/Depois) ou mudar o tamanho (grande, com o nome, ou
pequeno, só o ícone, metade da largura); embaixo, os blocos que faltam, com "+".
Em **Espaço entre os ícones da barra**, ainda na edição, os ícones no painel ficam mais
juntos ou mais afastados, para telas grandes: Pequeno (como antes), Médio ou Grande. O
popup não muda. Em **Ícones na barra** dá para esconder as notificações, a rede, o volume,
a bateria do computador e cada periférico (um mouse que sempre diz 100%, por exemplo), e
em **Tamanho dos ícones da barra**, deixá-los menores ou maiores.
O botão de desfazer no topo restaura o padrão. A escolha fica salva na configuração do COSMIC.

## Instalar

Pela COSMIC Store (Flatpak), que funciona em qualquer distribuição, ou a partir do código:

```sh
./install.sh            # instala em ~/.local
```

Pelo código, funciona no Fedora, no Debian e derivados (Ubuntu, Pop!_OS, Mint…) e no Arch e
derivados (Manjaro, EndeavourOS…). O `install.sh` chama o `deps.sh`, que instala com `dnf`,
`apt` ou `pacman` o que faltar para compilar e os programas que o applet usa (`wpctl`,
`pw-dump`, `pactl`, `curl`). O libcosmic pede Rust 1.93 ou mais novo, mais novo que o de
várias distribuições (Debian, Ubuntu e Pop!_OS LTS): se o do sistema for antigo, o script
avisa; instale o Rust pelo [rustup](https://rustup.rs) e rode de novo.

Os serviços que ele mostra ficam por conta da distribuição, e cada bloco só funciona com o
seu:

| Bloco | Precisa de | |
| --- | --- | --- |
| Wi-Fi | NetworkManager | sem ele (só `iwd` ou `systemd-networkd`, comum no Arch), o bloco não funciona |
| Bluetooth | BlueZ | |
| Bateria | UPower | |
| Energia | power-profiles-daemon, ou o tuned-ppd do Fedora | o Pop!_OS usa o `system76-power`; sem um serviço que responda como o power-profiles-daemon, o bloco não funciona |
| Som | PipeWire com WirePlumber | o padrão nas distribuições atuais |

### Flatpak

O manifesto fica em `flatpak/`; `cargo-sources.json` sai do `Cargo.lock` com o
[flatpak-cargo-generator](https://github.com/flatpak/flatpak-builder-tools/tree/master/cargo).
Na sandbox, o applet roda os programas do sistema (`wpctl`, `pactl`, `pw-dump`,
`cosmic-randr`, `ddcutil`, `cosmic-osd`, `curl`…) por `flatpak-spawn --host`, e por isso
pede `org.freedesktop.Flatpak`; o resto das permissões são os serviços que ele mostra e
controla (NetworkManager, BlueZ, UPower, perfis de energia, MPRIS, notificações) e as
configurações do COSMIC em `~/.config/cosmic`. Sem enxergar as câmeras de dentro da
sandbox, ele pergunta ao sistema quem as usa a cada 5 segundos.

O applet fala português, inglês e francês, conforme o idioma do sistema.

Depois adicione **Configurações rápidas** em Configurações › Área de trabalho › Painel ›
Applets. Ele não substitui os applets de rede, som, Bluetooth e bateria; dá para tirá-los
se ficar repetido.

## De onde vem cada coisa

| Controle | Fonte |
| --- | --- |
| Wi-Fi, rede em uso, VPN, DNS | NetworkManager |
| Bluetooth | BlueZ (primeiro adaptador) |
| Bateria, periféricos | UPower; no Bluetooth, também a bateria que o BlueZ informa |
| Energia | power-profiles-daemon (ou o tuned-ppd do Fedora) |
| Volume | PipeWire, por `wpctl`; mudanças chegam por `pactl subscribe` |
| Brilho | cosmic-settings-daemon |
| Não perturbe, Tema escuro | configurações do COSMIC, as mesmas que o Configurações usa |
| Modo avião | desliga Wi-Fi e Bluetooth juntos |
| Microfone mudo | PipeWire, por `wpctl` |
| Manter acordado | pede ao `cosmic-idle` para não apagar a tela nem suspender (`org.freedesktop.ScreenSaver`) |
| Lado a lado | protocolo de espaços de trabalho do COSMIC (no espaço atual) e a config do `cosmic-comp` (`autotile`, `active_hint`) |
| Lupa, Inverter cores | protocolo de acessibilidade do `cosmic-comp` |
| Leitor de tela | barramento de acessibilidade (`org.a11y.Bus`), que liga o Orca |
| Alto contraste | paleta de alto contraste nos temas claro e escuro |
| Captura de tela, Espaços de trabalho | atalhos: `cosmic-screenshot` e `cosmic-workspaces` |
| Saídas de som | `pactl` |
| Mídia | MPRIS (`org.mpris.MediaPlayer2.*`); capas da web são baixadas com `curl` para o cache |
| Notificações | cosmic-notifications, pelo socket que o painel dá aos applets de notificação |
| Borda ativa | a configuração do `cosmic-ext-hot-edge` (`enabled`), que o daemon dele acompanha |
| Telas, escala | `cosmic-randr` |
| Brilho de monitores externos | `ddcutil` (DDC/CI) |

O COSMIC 1.8 não tem luz noturna, por isso não há esse bloco. Ícones que o tema Cosmic
não tem (os de perfil de energia, por exemplo) vêm do Adwaita.
