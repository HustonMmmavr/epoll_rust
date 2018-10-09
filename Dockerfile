FROM rust:1.28

# copy your source tree
COPY ./httpd.conf /etc/httpd.conf
#COPY ./../../../http-test /www/static
COPY ./ ./


# CMD cat ./httpd.conf && cat /etc/httpd.conf
# build for release
RUN cargo build --release

EXPOSE 80 
# set the startup command to run your binary
CMD ["./target/release/epoll_rust"]

# sudo docker run -v /home/mavr/http-test/:/www/static -p 8080:80 test
