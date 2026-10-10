{
  lib,
  stdenv,
  fetchurl,
  dpkg,
  autoPatchelfHook,
  wrapGAppsHook3,
  webkitgtk_4_1,
  gtk3,
  libsoup_3,
  glib,
  glib-networking,
  gsettings-desktop-schemas,
  cairo,
  pango,
  gdk-pixbuf,
  librsvg,
  libayatana-appindicator,
  dbus,
  openssl,
  xdg-utils,
}:

let
  source = lib.importJSON ./source.json;
in
stdenv.mkDerivation {
  pname = "tmgr";
  inherit (source) version;

  src = fetchurl {
    url = "https://github.com/tmgr-dev/frontend/releases/download/desktop-v${source.version}/TMGR_${source.version}_amd64.deb";
    inherit (source) hash;
  };

  nativeBuildInputs = [
    dpkg
    autoPatchelfHook
    wrapGAppsHook3
  ];

  buildInputs = [
    webkitgtk_4_1
    gtk3
    libsoup_3
    glib
    glib-networking
    gsettings-desktop-schemas
    cairo
    pango
    gdk-pixbuf
    librsvg
    libayatana-appindicator
    dbus
    openssl
  ];

  runtimeDependencies = [ libayatana-appindicator ];

  unpackPhase = ''
    runHook preUnpack
    dpkg-deb -x $src .
    runHook postUnpack
  '';

  installPhase = ''
    runHook preInstall
    mkdir -p $out
    cp -r usr/bin usr/share $out/
    runHook postInstall
  '';

  postInstall = ''
    for f in $out/share/applications/*.desktop; do
      substituteInPlace "$f" --replace-quiet "Exec=tmgr" "Exec=$out/bin/tmgr"
    done
  '';

  preFixup = ''
    gappsWrapperArgs+=(
      --set TMGR_DISABLE_UPDATER 1
      --prefix PATH : ${lib.makeBinPath [ xdg-utils ]}
    )
  '';

  meta = {
    description = "TMGR desktop app";
    homepage = "https://tmgr.dev";
    license = lib.licenses.unfree;
    sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
    platforms = [ "x86_64-linux" ];
    mainProgram = "tmgr";
  };
}
