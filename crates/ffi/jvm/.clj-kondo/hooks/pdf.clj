(ns hooks.pdf
  "What clj-kondo sees of sa.ingenious.pdf's macros.")

(defmacro defkind
  "A def whose getters bind `args`, so clj-kondo checks each expression, and the leading
  `:extends` and `:realize` options too."
  [kind-name args & body]
  (let [options (take-while (comp #{:extends :realize} first) (partition 2 body))
        pairs   (drop (count options) (partition 2 body))]
    `(def ~kind-name
       (fn ~(if (= 2 (count args)) args [(first args) (gensym "_")])
         [~@(map second options) ~@(map second pairs)]))))
