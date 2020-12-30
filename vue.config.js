const currentTime = (new Date()).getTime()

module.exports = {
  runtimeCompiler: true,
	lintOnSave: process.env.NODE_ENV !== 'production',
  chainWebpack: config => {
    config.module
      .rule('images')
      .use('url-loader')
      .loader('url-loader')
      .tap(options => Object.assign(options, { limit: 10240 }))

		config.output
			.filename(`js/[name].[contenthash:8].${currentTime}.js`)
			.chunkFilename(`js/[name].[contenthash:8].${currentTime}.js`)
		config
			.plugin('extract-css')
			.tap(args => {
				args[0].filename = `css/[name].[contenthash:8].${currentTime}.css`
				args[0].chunkFilename = `css/[name].[contenthash:8].${currentTime}.css`
				args[0].allChunks = true
				return args
			})
  }
}
