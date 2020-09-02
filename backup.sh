#!/bin/bash

echo Clearing Terminal !!!!!!

sleep 1

clear

sleep 1

echo Making Directory For Termux Backup !!!!!!

sleep 1

mkdir /sdcard/download/termux-backup

sleep 1

echo Changing Directory To Termux Home Directory !!!!!!

sleep 1

cd ${HOME}

sleep 1

echo Backuping Termux Files !!!!!!

sleep 1

cp -rf * /sdcard/download/termux-backup

sleep 1

echo Buckup Termux Files Done !!!!!!
