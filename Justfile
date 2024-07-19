list-files:
    @for file in src/*; do \
        echo "$(basename $file):"; \
        echo; \
        cat $file; \
        echo; \
    done | wl-copy
