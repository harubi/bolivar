(ns hooks.pdf
  "What clj-kondo sees of sa.ingenious.pdf's macros.")

(defmacro defkind
  "A def whose getters bind `args`, so clj-kondo checks each expression."
  [kind-name args & body]
  (let [realize (when (= :realize (first body)) (second body))
        body    (if realize (drop 2 body) body)]
    `(def ~kind-name
       (fn ~(if (= 2 (count args)) args [(first args) (gensym "_")])
         [~realize ~@(map second (partition 2 body))]))))
