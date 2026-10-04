#!/usr/bin/env bash
# Original orchestration; upstream consumers remain outside the repository.
set -eu
W5D_ROOT=${1:?usage: build-oracles.sh external-work-directory}
W5D_REPO=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
W5D_QT="$W5D_ROOT/qt4-build/qt-everywhere-opensource-src-4.8.7"
python3 - "$W5D_ROOT" <<'PY'
import sys
from pathlib import Path
work=Path(sys.argv[1]);text=(work/'sources/lisa.txt').read_text()
appendix=text.split('\nf.c\n',1)[1]
source=appendix[appendix.index('#include'):].replace('\f','')
(work/'lisa-official-f.c').write_text(source)
PY
cc -O2 "$W5D_ROOT/lisa-official-f.c" -o "$W5D_ROOT/lisa-official-f"
g++ -O2 -fPIC -DQT_NO_DEBUG -DQT_GUI_LIB -DQT_CORE_LIB -DQT_SHARED -DQ_WS_QWS \
  -I"$W5D_QT/include" -I"$W5D_QT/include/QtCore" -I"$W5D_QT/include/QtGui" \
  -I"$W5D_QT/src/3rdparty/harfbuzz/src" -I"$W5D_QT/src/3rdparty/freetype/include" \
  -I"$W5D_QT/mkspecs/linux-g++" "$W5D_REPO/reports/w5.d/qt-driver.cpp" \
  "$W5D_QT/src/gui/.obj/release-shared-emb-x86_64/qfontengine_qws.o" \
  -L"$W5D_QT/lib" -Wl,-rpath,"$W5D_QT/lib" -lQtGui -lQtCore -o "$W5D_ROOT/qt-consumer"
