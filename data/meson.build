desktop_file = i18n.merge_file(
  input: 'eu.nimmerfort.blackbody.desktop.in',
  output: 'eu.nimmerfort.blackbody.desktop',
  type: 'desktop',
  po_dir: '../po',
  install: true,
  install_dir: join_paths(get_option('datadir'), 'applications')
)

desktop_utils = find_program('desktop-file-validate', required: false)
if desktop_utils.found()
  test('Validate desktop file', desktop_utils,
    args: [desktop_file]
  )
endif

appstream_file = i18n.merge_file(
  input: 'eu.nimmerfort.blackbody.metainfo.xml.in',
  output: 'eu.nimmerfort.blackbody.metainfo.xml',
  po_dir: '../po',
  install: true,
  install_dir: join_paths(get_option('datadir'), 'metainfo')
)

appstreamcli = find_program('appstreamcli', required: false)
if appstreamcli.found()
  test('Validate appstream file', appstreamcli,
    args: ['validate', '--no-net', appstream_file]
  )
endif

install_data('eu.nimmerfort.blackbody.gschema.xml',
  install_dir: join_paths(get_option('datadir'), 'glib-2.0/schemas')
)

install_data('eu.nimmerfort.blackbody.svg',
  install_dir: join_paths(get_option('datadir'), 'icons/hicolor/scalable/apps')
)

install_data('eu.nimmerfort.blackbody-symbolic.svg',
  install_dir: join_paths(get_option('datadir'), 'icons/hicolor/symbolic/apps')
)

compile_schemas = find_program('glib-compile-schemas', required: false)
if compile_schemas.found()
  test('Validate schema file', compile_schemas,
    args: ['--strict', '--dry-run', meson.current_source_dir()]
  )
endif
