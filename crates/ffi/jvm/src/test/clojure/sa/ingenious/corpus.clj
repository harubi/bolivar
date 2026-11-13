(ns sa.ingenious.corpus
  "The golden corpus in tests/jvm-golden: what the UniFFI binding returned for each case."
  (:require [clojure.edn :as edn]
            [clojure.java.io :as io])
  (:import [java.util.zip GZIPInputStream]))

(def repo (System/getProperty "bolivar.test.repo" "../../.."))

(defn files []
  (->> (.listFiles (io/file repo "tests/jvm-golden/corpus"))
       (filter #(.endsWith (.getName ^java.io.File %) ".edn.gz"))
       (sort-by #(.getName ^java.io.File %))))

(defn read-case [file]
  (with-open [r (java.io.PushbackReader. (io/reader (GZIPInputStream. (io/input-stream file))))]
    (edn/read r)))

(defn path [case] (str repo "/" (:path case)))
