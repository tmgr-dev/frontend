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
			.filename(`js/[name].[contenthash:8].js`)
			.chunkFilename(`js/[name].[contenthash:8].js`)
  },
	css: {
		extract: {
			filename: `css/[name].[contenthash:8].css`,
			chunkFilename: `css/[name].[contenthash:8].css`
		}
	}
}
