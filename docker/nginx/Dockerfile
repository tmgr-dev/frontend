FROM nginx:1.12

WORKDIR /var/www/html

COPY ./docker/nginx/conf.d /etc/nginx/conf.d
COPY ./ /var/www/html

CMD ["nginx", "-g", "daemon off;"]
